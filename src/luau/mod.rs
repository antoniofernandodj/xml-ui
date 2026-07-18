//! Comportamento de componente escrito em **Luau**, interpretado em tempo de
//! execução — sem etapa de compilação.
//!
//! O bloco `<script>` de um template guarda **Luau** (5.4, via [`mlua`]):
//! [`LuauComponent`] o carrega do arquivo e executa as funções quando uma ação
//! chega — nada é compilado, então mudar a lógica não exige recompilar o app.
//!
//! # Acesso ao contexto
//!
//! Cada função Luau enxerga uma tabela global `ctx` espelhando o
//! [`Context`] do motor. Ler `ctx.contador` devolve o valor
//! atual (string); atribuir `ctx.contador = ...` grava de volta. Como Luau
//! coage strings numéricas em aritmética, um contador é só:
//!
//! ```luau
//! function incrementar()
//!     ctx.contador = ctx.contador + 1
//! end
//! ```
//!
//! Depois que a função retorna, toda a tabela `ctx` é copiada de volta ao
//! contexto do motor, então os bindings `{contador}` da markup refletem a
//! mudança na próxima avaliação.
//!
//! Ações de `onChange` (inputs) chegam com o texto digitado: a função recebe
//! esse valor como primeiro argumento **e** na global `value`.
//!
//! ```luau
//! function definir_nome(v)
//!     ctx.nome = v          -- ou: ctx.nome = value
//! end
//! ```
//!
//! # Imports / módulos
//!
//! O `<script>` pode dividir a lógica em **bibliotecas** e importá-las com
//! `require`, mantendo cada peça encapsulada (um client de rede, utilitários,
//! etc.):
//!
//! ```luau
//! local http = require("net/http_client")   -- net/http_client.luau
//! local api  = http.new("https://api.exemplo")
//!
//! function carregar()
//!     local res = api:get("/dados")          -- o módulo pode usar fetch (async)
//!     if res.ok then ctx.dados = res.body end
//! end
//! ```
//!
//! `require("a.b")` procura `a/b.luau` (e `a/b/init.luau`) resolvido, na ordem:
//! diretório do template → `<dir>/lib` → cada caminho em `GLACIER_LUAU_PATH`
//! (separados por `:`). Módulos rodam no **mesmo** interpretador do componente,
//! então enxergam `fetch` e são carregados uma única vez (cacheados como no Luau
//! padrão). Ver `install_module_system`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::asset_source::{AssetSource, DiskAssets};
use crate::component::{
    Component, Context, FetchResult, PendingFetch, PendingTimer, StreamCommand, StreamCommandKind,
    StreamEventKind, StreamKind, StreamRequest, Template,
};
use crate::error::{GlacierError, Result};
use mlua::{
    Function, Lua, LuaSerdeExt, MultiValue, RegistryKey, Table, Thread, ThreadStatus, Value,
};

/// Prelúdio Lua injetado antes do `<script>` do usuário. Define `fetch` e,
/// para streams de vida longa, `sse` / `websocket` (ver [`LuauComponent::drive`]).
///
/// `fetch` **suspende** a corrotina até a resposta (aparência de `await`).
/// `sse`/`websocket` **não** suspendem: cedem um pedido de abertura, o motor
/// registra o stream e retoma na hora devolvendo um handle. A partir daí os
/// eventos chegam pelos handlers nomeados em `opts` (`on_message`, `on_open`,
/// `on_error`, `on_close`), e o handle permite enviar/fechar:
///
/// ```luau
/// function init()
///     conn = websocket("wss://ex/ws", { on_message = "on_msg" })
/// end
/// function on_msg(data) ctx.ultima = data end
/// function enviar() conn:send(ctx.texto) end
/// ```
const PRELUDE: &str = include_str!("prelude.luau");

/// Um [`Component`] cujo comportamento vem de um bloco `<script>` em Luav.
///
/// O template XML é lido do disco; seu `<script>` é extraído e
/// carregado num interpretador Luau próprio. Cada ação (`on_click`, `onChange`,
/// `onSubmit`) roda como uma **corrotina**: chama a função Luau homônima, que
/// lê/escreve o contexto via a tabela global `ctx` e pode chamar `fetch` para
/// rede sem bloquear a UI.
pub struct LuauComponent {
    name: String,
    path: String,
    luau: Lua,
    /// Tabela `ctx` persistente (o mesmo objeto entre chamadas), espelhando o
    /// contexto do motor. Mantida fixa para que corrotinas suspensas que a
    /// referenciam continuem válidas ao serem retomadas.
    ctx_table: Table,
    /// Corrotinas suspensas num `fetch`, aguardando a resposta, por `id`.
    pending: RefCell<HashMap<u64, Thread>>,
    /// Streams de vida longa abertos (`sse`/`websocket`), por `id`: os handlers
    /// Lua registrados (`on_message`, …) que o motor chama a cada evento.
    streams: RefCell<HashMap<u64, StreamRegistration>>,
    /// Temporizadores (`after`) agendados e ainda não disparados/cancelados,
    /// por `id`: o handler Lua a chamar quando o motor retomar via
    /// [`Component::resume_timer`].
    timers: RefCell<HashMap<u64, RegistryKey>>,
    /// Gerador de `id`, compartilhado por `fetch`es, streams e timers (ids
    /// únicos no componente).
    next_id: Cell<u64>,
    /// Tabela `__glacier_viewport` persistente (o mesmo objeto entre
    /// chamadas), atualizada em [`Self::sync_to_luau`] com o viewport atual do
    /// motor — é o que o `viewport()` do prelúdio lê.
    viewport_table: Table,
}

/// Handlers Lua de um stream aberto, guardados como referências no registry do
/// interpretador (resolvidas na hora do evento). Um slot `None` = sem handler
/// para aquele evento.
struct StreamRegistration {
    on_open: Option<RegistryKey>,
    on_message: Option<RegistryKey>,
    on_error: Option<RegistryKey>,
    on_close: Option<RegistryKey>,
}

impl StreamRegistration {
    /// A referência do handler para um dado tipo de evento, se registrada.
    fn handler(&self, kind: StreamEventKind) -> Option<&RegistryKey> {
        match kind {
            StreamEventKind::Open => self.on_open.as_ref(),
            StreamEventKind::Message => self.on_message.as_ref(),
            StreamEventKind::Error => self.on_error.as_ref(),
            StreamEventKind::Closed => self.on_close.as_ref(),
        }
    }
}

impl LuauComponent {
    /// Cria um componente Luau a partir de um arquivo de template.
    ///
    /// O corpo Luau vem de uma de duas fontes:
    /// - **externo**: `<script src="arquivo.luau">` (ou `from="..."`) carrega o
    ///   Luau de outro arquivo, resolvido relativo ao diretório do template;
    /// - **inline**: senão, o corpo do próprio bloco `<script>...</script>`.
    ///
    /// O script é executado uma vez para definir as funções. Erros de I/O ou de
    /// sintaxe Luau viram [`GlacierError::Luau`], já anotados com o componente
    /// dono — quem chama daqui de fora recebe o mesmo tipo de erro que o resto da
    /// API, não uma `String` solta.
    ///
    /// (Por dentro, a construção segue trocando `String`: as mensagens vêm do
    /// mlua e já dizem arquivo e linha *do Luau*, que é a informação que importa.
    /// O tipo entra na fronteira pública, que é onde ele tem valor.)
    pub fn from_file(path: impl Into<String>, name: impl Into<String>) -> Result<Self> {
        Self::from_file_with(path, name, Arc::new(DiskAssets))
    }

    /// Como [`LuauComponent::from_file`], mas lendo o template, o `<script src>`
    /// externo e os módulos `require`d através de um [`AssetSource`] — o que
    /// permite um binário standalone com os scripts embutidos. A variante sem
    /// `assets` usa [`DiskAssets`].
    pub fn from_file_with(
        path: impl Into<String>,
        name: impl Into<String>,
        assets: Arc<dyn AssetSource>,
    ) -> Result<Self> {
        let path = path.into();
        let name = name.into();
        Self::from_file_inner(&path, &name, &assets).map_err(|message| GlacierError::Luau {
            component: name,
            message,
        })
    }

    fn from_file_inner(
        path: &str,
        name: &str,
        assets: &Arc<dyn AssetSource>,
    ) -> std::result::Result<Self, String> {
        let content = assets
            .read_to_string(path)
            .map_err(|e| format!("Falha ao ler template Luau em '{}': {}", path, e))?;
        let (script, script_path) = resolve_script(&content, path, assets.as_ref())?;
        // `require` de um `<script src>` EXTERNO resolve relativo ao diretório do
        // SCRIPT (permite separar `views/` de `views/scripts/` e ainda
        // `require("net/api")` a partir do script); inline, relativo ao template.
        let module_base = script_path.unwrap_or_else(|| PathBuf::from(path));
        Self::build(
            &script,
            path.to_string(),
            name.to_string(),
            &module_base,
            assets.clone(),
        )
    }

    /// Cria um componente Luau a partir do código-fonte já extraído, associando-o
    /// a um `path` de template (para o motor renderizar a UI e manter hot-reload).
    /// Resolve `require` relativo ao diretório do próprio `path`.
    pub fn from_source(
        script: &str,
        path: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Self> {
        Self::from_source_with(script, path, name, Arc::new(DiskAssets))
    }

    /// Como [`LuauComponent::from_source`], mas resolvendo `require` através de
    /// um [`AssetSource`] (módulos embutidos). A variante sem `assets` usa
    /// [`DiskAssets`].
    pub fn from_source_with(
        script: &str,
        path: impl Into<String>,
        name: impl Into<String>,
        assets: Arc<dyn AssetSource>,
    ) -> Result<Self> {
        let path = path.into();
        let name = name.into();
        let base = PathBuf::from(&path);
        Self::build(script, path, name.clone(), &base, assets).map_err(|message| {
            GlacierError::Luau {
                component: name,
                message,
            }
        })
    }

    /// Núcleo compartilhado: `module_base` é o arquivo cujo diretório ancora a
    /// resolução de `require` (o script externo, ou o template para inline).
    fn build(
        script: &str,
        path: String,
        name: String,
        module_base: &Path,
        assets: Arc<dyn AssetSource>,
    ) -> std::result::Result<Self, String> {
        let luau = Lua::new();
        luau.load(PRELUDE)
            .set_name("<glacier prelude>")
            .exec()
            .map_err(|e| format!("Erro ao carregar prelúdio Luau: {}", e))?;
        // Habilita `require(...)` resolvendo módulos relativo ao script/template.
        install_module_system(&luau, module_roots(module_base), assets)
            .map_err(|e| format!("Erro ao instalar sistema de módulos Luau: {}", e))?;
        // Expõe o global `json` (encode/decode) para o script e seus módulos —
        // essencial para consumir/produzir os payloads JSON das APIs via `fetch`.
        install_json(&luau).map_err(|e| format!("Erro ao instalar `json` Luau: {}", e))?;
        // Expõe o global `storage` (persistência local em JSON, análoga a
        // `localStorage`), num arquivo próprio deste componente.
        install_storage(
            &luau,
            storage_path(module_base, &name, STORAGE_ROOT.get().map(|p| p.as_path())),
        )
            .map_err(|e| format!("Erro ao instalar `storage` Luau: {}", e))?;
        // Expõe o global `write_file(path, conteúdo)` (escrita de arquivo local).
        // A leitura correspondente é `fetch("file://…")` (ver `net::perform`).
        install_write_file(&luau)
            .map_err(|e| format!("Erro ao instalar `write_file` Luau: {}", e))?;
        // Tabela persistente que `viewport()` (prelúdio) lê — populada a cada
        // execução em `sync_to_luau`.
        let viewport_table = luau
            .create_table()
            .map_err(|e| format!("Erro ao criar tabela de viewport: {}", e))?;
        viewport_table.set("width", 0.0).ok();
        viewport_table.set("height", 0.0).ok();
        luau.globals()
            .set("__glacier_viewport", &viewport_table)
            .map_err(|e| format!("Erro ao registrar __glacier_viewport: {}", e))?;
        // A tabela `ctx` precisa existir ANTES de rodar o script do usuário: o
        // corpo de topo pode referenciá-la (ex.: `Console.new(ctx)`). Ela é o
        // mesmo objeto entre chamadas (só limpa/repopulada em `sync_to_luau`),
        // então uma referência capturada aqui no load continua válida.
        let ctx_table = luau
            .create_table()
            .map_err(|e| format!("Erro ao criar tabela ctx: {}", e))?;
        luau.globals()
            .set("ctx", &ctx_table)
            .map_err(|e| format!("Erro ao registrar ctx: {}", e))?;
        luau.load(script)
            .set_name(format!("<script:{name}>"))
            .exec()
            .map_err(|e| format!("Erro ao carregar <script> Luau de '{}': {}", name, e))?;
        Ok(Self {
            name,
            path,
            luau,
            ctx_table,
            pending: RefCell::new(HashMap::new()),
            streams: RefCell::new(HashMap::new()),
            timers: RefCell::new(HashMap::new()),
            next_id: Cell::new(1),
            viewport_table,
        })
    }

    /// Espelha o contexto do motor na tabela Luau `ctx`: limpa a tabela e a
    /// repopula com o estado atual, para que ela reflita o contexto *exatamente*
    /// no início da execução. É o que permite ao `sync_from_luau` detectar o que
    /// o Luau removeu (`ctx.x = nil`). A tabela é limpa in-place (mesmo objeto),
    /// preservando referências de corrotinas suspensas.
    fn sync_to_luau(&self, ctx: &Context) -> mlua::Result<()> {
        self.ctx_table.clear()?;
        for (k, v) in ctx.data.iter() {
            self.ctx_table.set(k.as_str(), v.as_str())?;
        }
        self.viewport_table.set("width", ctx.viewport.0)?;
        self.viewport_table.set("height", ctx.viewport.1)?;
        Ok(())
    }

    /// Copia a tabela `ctx` de volta ao contexto do motor, tratando-a como a
    /// fonte da verdade: chaves com valor string-izável são gravadas (novas
    /// incluídas); chaves que o Luau apagou (`ctx.x = nil`) são **removidas** do
    /// contexto — como a tabela começou espelhando o contexto (ver
    /// [`Self::sync_to_luau`]), toda chave do contexto ausente aqui foi
    /// deliberadamente removida pelo script. `nil`/tabelas/funções não são
    /// gravados.
    fn sync_from_luau(&self, ctx: &mut Context) -> mlua::Result<()> {
        let mut present = std::collections::HashSet::new();
        for pair in self.ctx_table.pairs::<String, Value>() {
            let (k, val) = pair?;
            present.insert(k.clone());
            match luau_value_to_string(&self.luau, &val) {
                Some(s) => ctx.set(&k, s),
                // `nil` é remoção deliberada (tratada abaixo via `present`);
                // qualquer outro tipo que não virou string é descartado, mas
                // avisado — ao contrário do silêncio total de antes.
                None if !matches!(val, Value::Nil) => eprintln!(
                    "[glacier-ui] aviso: <script> Lua '{}' descartou ctx.{} ao sincronizar \
                     (valor do tipo '{}' não é serializável — funções/threads/userdata não \
                     podem ir para o contexto; tabelas com esses tipos dentro também falham)",
                    self.name,
                    k,
                    val.type_name()
                ),
                None => {}
            }
        }
        // Chaves que existiam no contexto mas não estão mais na tabela (o Luau as
        // setou para nil) são removidas.
        let removed: Vec<String> = ctx
            .data
            .keys()
            .filter(|k| !present.contains(*k))
            .cloned()
            .collect();
        for k in removed {
            ctx.data.remove(&k);
        }
        Ok(())
    }

    /// Roda a função `func` (se existir) como uma corrotina, passando `value`.
    fn run(&self, func: &str, value: Option<&str>, ctx: &mut Context) {
        if let Err(e) = self.run_inner(func, value, ctx) {
            self.report_error(func, e, ctx);
        }
    }

    /// Relata um erro de execução do script: sempre loga em `stderr` (o
    /// equivalente ao console do DevTools) e, além disso —
    ///
    /// - se o script define um `on_error(msg)` global, chama-o como
    ///   corrotina (pode, ele mesmo, mostrar um `toast`/abrir um `confirm`);
    /// - senão, promove automaticamente a mensagem a um toast de erro, para
    ///   que o usuário final veja *algo* em vez de "o botão não faz nada".
    ///
    /// Nunca propaga: se o próprio `on_error` falhar, só loga (evita loop).
    fn report_error(&self, where_: &str, err: impl std::fmt::Display, ctx: &mut Context) {
        let msg = format!(
            "[glacier-ui] erro em <script> Lua '{}::{}': {}",
            self.name, where_, err
        );
        eprintln!("{msg}");
        match self.luau.globals().get::<Function>("on_error") {
            Ok(f) => {
                if let Ok(thread) = self.luau.create_thread(f) {
                    let arg = match self.luau.create_string(&msg) {
                        Ok(s) => Value::String(s),
                        Err(_) => Value::Nil,
                    };
                    if let Err(e2) = self.drive(thread, MultiValue::from_iter([arg]), ctx) {
                        eprintln!(
                            "[glacier-ui] erro dentro do próprio on_error de '{}': {}",
                            self.name, e2
                        );
                    }
                }
            }
            Err(_) => {
                ctx.show_toast(crate::toasts::ToastSpec::error(msg).with_title("Erro de script"));
            }
        }
    }

    fn run_inner(&self, func: &str, value: Option<&str>, ctx: &mut Context) -> mlua::Result<()> {
        self.sync_to_luau(ctx)?;
        self.luau.globals().set("value", value)?;

        // Resolve a ação para uma função global. Primeiro tenta o nome exato;
        // se não houver e a ação for `nome:sufixo`, cai para `nome(sufixo, value)`
        // — a convenção que templates parametrizados usam para ações por-linha
        // (`open_service:<id>`, `field:<chave>`, `proj_tab:<aba>`), espelhando o
        // que um componente Rust faria fatiando a própria string.
        //
        // Fallback final: uma ação simples (sem função e sem ':') que carrega um
        // `value` é tratada como binding de input — grava `ctx[ação] = value`.
        // É o que fecha o loop de um `formControl="url"` (cujo onChange implícito
        // é o próprio nome do controle) sem exigir um handler por campo, papel
        // que o `Form` do Rust cumpria via `sync_to_context`. Ações sem função
        // e sem value continuam ignoradas (como o `_ => {}` antigo).
        let globals = self.luau.globals();
        let (f, lead) = match globals.get::<Function>(func) {
            Ok(f) => (f, None),
            Err(_) => match func.split_once(':') {
                Some((name, suffix)) => match globals.get::<Function>(name) {
                    Ok(f) => (f, Some(suffix.to_string())),
                    Err(_) => return Ok(()),
                },
                None => {
                    if let Some(v) = value {
                        ctx.set(func, v);
                    }
                    return Ok(());
                }
            },
        };

        let thread = self.luau.create_thread(f)?;
        // Args na ordem: [sufixo?, value?]. Para `nome:sufixo` o sufixo vem
        // primeiro; o `value` de um onChange/onToggle segue (permite
        // `field(chave, texto)`). Sem sufixo, só o `value` (comportamento antigo).
        let mut items: Vec<Value> = Vec::new();
        if let Some(s) = &lead {
            items.push(Value::String(self.luau.create_string(s)?));
        }
        if let Some(v) = value {
            items.push(Value::String(self.luau.create_string(v)?));
        }
        let args = MultiValue::from_iter(items);
        self.drive(thread, args, ctx)
    }

    /// Retoma a corrotina suspensa `id` com o resultado do `fetch`.
    fn resume_inner(&self, id: u64, result: &FetchResult, ctx: &mut Context) -> mlua::Result<()> {
        let Some(thread) = self.pending.borrow_mut().remove(&id) else {
            return Ok(());
        };
        self.sync_to_luau(ctx)?;
        let res = self.result_to_luau(result)?;
        self.drive(thread, MultiValue::from_iter([Value::Table(res)]), ctx)
    }

    /// Retoma a corrotina suspensa num `confirm()` (id alocado no `drive`) com a
    /// escolha do usuário — `true` confirmou, `false` cancelou/dispensou —, que
    /// vira o valor de retorno do `coroutine.yield` no prelúdio (`local ok =
    /// confirm{...}`).
    fn resume_dialog_inner(&self, id: u64, confirmed: bool, ctx: &mut Context) -> mlua::Result<()> {
        let Some(thread) = self.pending.borrow_mut().remove(&id) else {
            return Ok(());
        };
        self.sync_to_luau(ctx)?;
        self.drive(thread, MultiValue::from_iter([Value::Boolean(confirmed)]), ctx)
    }

    /// Dispara o handler de um temporizador (`after`) vencido: se ainda
    /// registrado (não cancelado, não já disparado — é de disparo único),
    /// chama-o numa corrotina nova, como um evento de stream.
    fn resume_timer_inner(&self, id: u64, ctx: &mut Context) -> mlua::Result<()> {
        let Some(key) = self.timers.borrow_mut().remove(&id) else {
            return Ok(()); // cancelado, ou id desconhecido
        };
        let func: Function = self.luau.registry_value(&key)?;
        self.sync_to_luau(ctx)?;
        let thread = self.luau.create_thread(func)?;
        self.drive(thread, MultiValue::new(), ctx)
    }

    /// Aloca o próximo `id` (único no componente, compartilhado por fetch/stream).
    fn alloc_id(&self) -> u64 {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        id
    }

    /// Guia a corrotina até ela terminar ou suspender num `fetch`. Cada `yield`
    /// carrega um pedido que o motor entende:
    ///
    /// - `__glacier_fetch`: registra a requisição, guarda a corrotina em
    ///   `pending` e **para** (retomada depois via [`Self::resume_inner`]).
    /// - `__glacier_stream_open`: abre um stream, registra os handlers e
    ///   **retoma na hora** devolvendo o `id` (o Luau recebe o handle).
    /// - `__glacier_stream_cmd`: registra o comando de saída (`send`/`close`) e
    ///   **retoma na hora**.
    ///
    /// Só `fetch` suspende de verdade; stream-open/cmd continuam o mesmo turno.
    fn drive(&self, thread: Thread, mut args: MultiValue, ctx: &mut Context) -> mlua::Result<()> {
        loop {
            let yielded: MultiValue = thread.resume(args)?;
            self.sync_from_luau(ctx)?;

            if thread.status() != ThreadStatus::Resumable {
                return Ok(()); // corrotina terminou
            }

            let Some(Value::Table(req)) = yielded.into_iter().next() else {
                return Ok(()); // yield que o motor não entende: deixa parada
            };

            if req.get::<bool>("__glacier_fetch").unwrap_or(false) {
                let id = self.alloc_id();
                ctx.fetches.push(self.parse_fetch(id, &req)?);
                self.pending.borrow_mut().insert(id, thread);
                return Ok(()); // suspende até a resposta
            }

            if req.get::<bool>("__glacier_stream_open").unwrap_or(false) {
                let id = self.alloc_id();
                self.register_stream(id, &req, ctx)?;
                // Retoma devolvendo o id como handle e segue no mesmo turno.
                args = MultiValue::from_iter([Value::Integer(id as i64)]);
                continue;
            }

            if req.get::<bool>("__glacier_stream_cmd").unwrap_or(false) {
                self.record_stream_cmd(&req, ctx)?;
                args = MultiValue::new();
                continue;
            }

            if req.get::<bool>("__glacier_dialog").unwrap_or(false) {
                // Suspende igual ao `fetch`: guarda a corrotina e para. O motor
                // exibe o diálogo e, quando o usuário clica num botão, retoma
                // esta corrotina com o booleano da escolha (ver
                // `resume_dialog_inner`) — é o que dá a `confirm()` a aparência
                // síncrona (`local ok = confirm{...}`).
                let id = self.alloc_id();
                ctx.show_dialog_resumable(build_dialog(&req)?, id);
                self.pending.borrow_mut().insert(id, thread);
                return Ok(()); // suspende até a escolha do usuário
            }

            if req.get::<bool>("__glacier_toast").unwrap_or(false) {
                ctx.show_toast(build_toast(&req)?);
                args = MultiValue::new();
                continue;
            }

            if req.get::<bool>("__glacier_notify").unwrap_or(false) {
                ctx.notify(build_notification(&req)?);
                args = MultiValue::new();
                continue;
            }

            if req.get::<bool>("__glacier_nav").unwrap_or(false) {
                let screen: String = req.get("screen")?;
                ctx.navigate_to(&screen);
                args = MultiValue::new();
                continue;
            }

            if req.get::<bool>("__glacier_nav_back").unwrap_or(false) {
                ctx.navigate_back();
                args = MultiValue::new();
                continue;
            }

            if req.get::<bool>("__glacier_window").unwrap_or(false) {
                ctx.open_window(build_window_spec(&self.luau, &req)?);
                args = MultiValue::new();
                continue;
            }

            if req.get::<bool>("__glacier_broadcast").unwrap_or(false) {
                let event: String = req.get("event")?;
                // `payload` é opcional; uma tabela é serializada em JSON (o mesmo
                // caminho de `ctx.foo = tabela`), uma string vai como está, `nil`
                // vira "".
                let payload = match req.get::<Value>("payload")? {
                    Value::Nil => String::new(),
                    Value::String(s) => s.to_string_lossy(),
                    other => luau_value_to_string(&self.luau, &other).unwrap_or_default(),
                };
                ctx.broadcast(event, payload);
                args = MultiValue::new();
                continue;
            }

            if req.get::<bool>("__glacier_window_close").unwrap_or(false) {
                ctx.close_window();
                args = MultiValue::new();
                continue;
            }

            if req.get::<bool>("__glacier_editor_append").unwrap_or(false) {
                let binding: String = req.get("binding")?;
                let text: String = req.get("text")?;
                ctx.append_textarea(binding, text);
                args = MultiValue::new();
                continue;
            }

            if req.get::<bool>("__glacier_after").unwrap_or(false) {
                let id = self.alloc_id();
                let ms: u64 = req.get("ms")?;
                if let Some(key) = self.resolve_handler_value(req.get("fn")?)? {
                    self.timers.borrow_mut().insert(id, key);
                }
                ctx.timers.push(PendingTimer::new(id, ms));
                // Retoma devolvendo o id como handle (cancelável) — não bloqueia
                // como `fetch`, no mesmo espírito de `sse`/`websocket`.
                args = MultiValue::from_iter([Value::Integer(id as i64)]);
                continue;
            }

            if req.get::<bool>("__glacier_after_cancel").unwrap_or(false) {
                let id: u64 = req.get("id")?;
                self.timers.borrow_mut().remove(&id);
                args = MultiValue::new();
                continue;
            }

            return Ok(()); // tabela cedida sem marcador conhecido
        }
    }

    /// Registra um stream pedido por `sse`/`websocket`: lê `kind`, `url`, os
    /// headers e os handlers nomeados de `opts`, guarda os handlers e empurra o
    /// [`StreamRequest`] para o motor abrir a conexão.
    fn register_stream(&self, id: u64, req: &Table, ctx: &mut Context) -> mlua::Result<()> {
        let kind = match req.get::<String>("kind")?.as_str() {
            "ws" => StreamKind::Ws,
            _ => StreamKind::Sse,
        };
        let url: String = req.get("url")?;
        let opts: Option<Table> = req.get("opts")?;
        let (headers, reg) = match opts {
            Some(o) => {
                let headers = parse_headers_table(&o)?;
                let reg = StreamRegistration {
                    on_open: self.handler_key(&o, "on_open")?,
                    on_message: self.handler_key(&o, "on_message")?,
                    on_error: self.handler_key(&o, "on_error")?,
                    on_close: self.handler_key(&o, "on_close")?,
                };
                (headers, reg)
            }
            None => (
                Vec::new(),
                StreamRegistration {
                    on_open: None,
                    on_message: None,
                    on_error: None,
                    on_close: None,
                },
            ),
        };
        self.streams.borrow_mut().insert(id, reg);
        ctx.streams.push(StreamRequest::new(id, kind, url, headers));
        Ok(())
    }

    /// Resolve um handler de `opts[name]` num [`RegistryKey`]: aceita uma
    /// função direta ou o **nome** de uma função global (resolvida agora). Um
    /// nome sem global correspondente, ou um valor de outro tipo, vira `None`.
    fn handler_key(&self, opts: &Table, name: &str) -> mlua::Result<Option<RegistryKey>> {
        match opts.get::<Value>(name)? {
            Value::Function(f) => Ok(Some(self.luau.create_registry_value(f)?)),
            Value::String(s) => match self.luau.globals().get::<Value>(s.to_string_lossy())? {
                Value::Function(f) => Ok(Some(self.luau.create_registry_value(f)?)),
                _ => Ok(None),
            },
            _ => Ok(None),
        }
    }

    /// Resolve um valor de handler (o `fn` de `after(ms, fn)`) num
    /// [`RegistryKey`]: aceita uma função direta ou o **nome** de uma função
    /// global (resolvida agora), igual a [`Self::handler_key`] mas para um
    /// valor solto em vez de um campo de `opts`. Um nome sem global
    /// correspondente, ou um valor de outro tipo, vira `None` (o
    /// temporizador ainda dispara, só não chama nada).
    fn resolve_handler_value(&self, v: Value) -> mlua::Result<Option<RegistryKey>> {
        match v {
            Value::Function(f) => Ok(Some(self.luau.create_registry_value(f)?)),
            Value::String(s) => match self.luau.globals().get::<Value>(s.to_string_lossy())? {
                Value::Function(f) => Ok(Some(self.luau.create_registry_value(f)?)),
                _ => Ok(None),
            },
            _ => Ok(None),
        }
    }

    /// Extrai o [`StreamCommand`] (`send`/`close`) que o handle Luau cedeu.
    fn record_stream_cmd(&self, req: &Table, ctx: &mut Context) -> mlua::Result<()> {
        let id: u64 = req.get("id")?;
        let kind = match req.get::<String>("cmd")?.as_str() {
            "close" => StreamCommandKind::Close,
            _ => StreamCommandKind::Send,
        };
        let text: Option<String> = req.get("text")?;
        ctx.stream_cmds
            .push(StreamCommand::new(id, kind, text.unwrap_or_default()));
        Ok(())
    }

    /// Entrega um evento de stream ao handler Luau registrado (se houver),
    /// rodando-o como corrotina (pode, ele mesmo, chamar `fetch`/abrir streams).
    /// No `Closed`, descarta o registro do stream (e suas refs de handler).
    fn on_stream_event_inner(
        &self,
        id: u64,
        kind: StreamEventKind,
        data: &str,
        ctx: &mut Context,
    ) -> mlua::Result<()> {
        let func: Option<Function> = {
            let streams = self.streams.borrow();
            match streams.get(&id).and_then(|r| r.handler(kind)) {
                Some(key) => Some(self.luau.registry_value(key)?),
                None => None,
            }
        };
        if kind == StreamEventKind::Closed {
            self.streams.borrow_mut().remove(&id);
        }
        let Some(func) = func else { return Ok(()) };

        self.sync_to_luau(ctx)?;
        self.luau.globals().set("value", data)?;
        let thread = self.luau.create_thread(func)?;
        let args = MultiValue::from_iter([Value::String(self.luau.create_string(data)?)]);
        self.drive(thread, args, ctx)
    }

    /// Entrega um broadcast de outra janela chamando a função Lua global
    /// `on_broadcast(event, payload)`. `payload` (JSON) é decodificado para um
    /// valor Lua (tabela) antes da chamada; vazio vira `nil`, JSON inválido vira
    /// a string crua. Janelas sem `on_broadcast` global ignoram (no-op).
    fn on_broadcast_inner(
        &self,
        event: &str,
        payload: &str,
        ctx: &mut Context,
    ) -> mlua::Result<()> {
        let Ok(func) = self.luau.globals().get::<Function>("on_broadcast") else {
            return Ok(());
        };
        self.sync_to_luau(ctx)?;
        let payload_val: Value = if payload.is_empty() {
            Value::Nil
        } else {
            match serde_json::from_str::<serde_json::Value>(payload) {
                Ok(json) => self.luau.to_value(&json)?,
                Err(_) => Value::String(self.luau.create_string(payload)?),
            }
        };
        let thread = self.luau.create_thread(func)?;
        let args =
            MultiValue::from_iter([Value::String(self.luau.create_string(event)?), payload_val]);
        self.drive(thread, args, ctx)
    }

    /// Extrai uma [`PendingFetch`] da tabela `{ url, opts }` que o `fetch` cedeu.
    fn parse_fetch(&self, id: u64, req: &Table) -> mlua::Result<PendingFetch> {
        let url: String = req.get("url")?;
        let opts: Option<Table> = req.get("opts")?;
        let (method, body, headers) = match opts {
            Some(o) => {
                let method = o
                    .get::<Option<String>>("method")?
                    .unwrap_or_else(|| "GET".into());
                let body = o.get::<Option<String>>("body")?;
                let mut headers = parse_headers_table(&o)?;
                // Atalho `user_agent = "..."`: vira um header User-Agent, a menos
                // que o chamador já tenha posto um em `headers` (esse vence). Sem
                // isto, o net aplica o UA padrão (ver DEFAULT_USER_AGENT).
                if let Some(ua) = o.get::<Option<String>>("user_agent")?
                    && !headers
                        .iter()
                        .any(|(k, _)| k.eq_ignore_ascii_case("user-agent"))
                {
                    headers.push(("User-Agent".to_string(), ua));
                }
                (method, body, headers)
            }
            None => ("GET".into(), None, Vec::new()),
        };
        Ok(PendingFetch::new(id, url, method, body, headers))
    }

    /// Converte um [`FetchResult`] na tabela Luau `{ ok, status, body, error }`.
    fn result_to_luau(&self, r: &FetchResult) -> mlua::Result<Table> {
        let t = self.luau.create_table()?;
        t.set("ok", r.ok)?;
        t.set("status", r.status)?;
        t.set("body", r.body.as_str())?;
        t.set("error", r.error.as_str())?;
        Ok(t)
    }
}

impl Component for LuauComponent {
    fn name(&self) -> &str {
        &self.name
    }

    fn template(&self) -> Template {
        Template::File(self.path.clone())
    }

    /// Chama uma função Luau opcional `init()` para semear o estado inicial.
    fn init(&mut self, ctx: &mut Context) {
        self.run("init", None, ctx);
    }

    fn update(&mut self, action: &str, value: Option<&str>, ctx: &mut Context) {
        self.run(action, value, ctx);
    }

    fn on_form_submit(&mut self, action: &str, ctx: &mut Context) {
        self.run(action, None, ctx);
    }

    fn resume_fetch(&mut self, id: u64, result: &FetchResult, ctx: &mut Context) {
        if let Err(e) = self.resume_inner(id, result, ctx) {
            self.report_error(&format!("fetch #{id}"), e, ctx);
        }
    }

    fn resume_dialog(&mut self, id: u64, confirmed: bool, ctx: &mut Context) {
        if let Err(e) = self.resume_dialog_inner(id, confirmed, ctx) {
            self.report_error(&format!("confirm #{id}"), e, ctx);
        }
    }

    fn on_stream_event(&mut self, id: u64, kind: StreamEventKind, data: &str, ctx: &mut Context) {
        if let Err(e) = self.on_stream_event_inner(id, kind, data, ctx) {
            self.report_error(&format!("stream #{id}"), e, ctx);
        }
    }

    fn on_broadcast(&mut self, event: &str, payload: &str, ctx: &mut Context) {
        if let Err(e) = self.on_broadcast_inner(event, payload, ctx) {
            self.report_error(&format!("on_broadcast '{event}'"), e, ctx);
        }
    }

    fn resume_timer(&mut self, id: u64, ctx: &mut Context) {
        if let Err(e) = self.resume_timer_inner(id, ctx) {
            self.report_error(&format!("after #{id}"), e, ctx);
        }
    }
}

/// Lê a subtabela `headers` de uma tabela de opções (`fetch`/`sse`/`websocket`)
/// como uma lista de pares `(nome, valor)`. Ausente/`nil` devolve vazio.
fn parse_headers_table(opts: &Table) -> mlua::Result<Vec<(String, String)>> {
    let mut headers = Vec::new();
    if let Some(h) = opts.get::<Option<Table>>("headers")? {
        for pair in h.pairs::<String, String>() {
            let (k, v) = pair?;
            headers.push((k, v));
        }
    }
    Ok(headers)
}

/// Constrói o [`DialogSpec`] a partir do pedido `confirm(opts)` do prelúdio:
/// dois botões (cancelar neutro → retoma com `false`; confirmar → retoma com
/// `true`), não dispensável clicando fora. Os botões carregam as ações
/// sentinela [`CONFIRM_NO`]/[`CONFIRM_YES`] em vez de nomes de função — o motor
/// as reconhece e retoma a corrotina suspensa (ver
/// [`crate::component::DialogAction::ShowResumable`]) em vez de despachá-las.
/// `destructive` pinta o botão de confirmação como perigo.
fn build_dialog(req: &Table) -> mlua::Result<crate::dialogs::DialogSpec> {
    use crate::dialogs::{ButtonRole, DialogButton, DialogIcon, DialogSpec, CONFIRM_NO, CONFIRM_YES};
    let title: String = req.get::<Option<String>>("title")?.unwrap_or_default();
    let message: String = req.get::<Option<String>>("message")?.unwrap_or_default();
    let confirm_label = req
        .get::<Option<String>>("confirm_label")?
        .unwrap_or_else(|| "OK".into());
    let cancel_label = req
        .get::<Option<String>>("cancel_label")?
        .unwrap_or_else(|| "Cancelar".into());
    let destructive = req.get::<Option<bool>>("destructive")?.unwrap_or(false);
    let role = if destructive {
        ButtonRole::Destructive
    } else {
        ButtonRole::Accept
    };
    Ok(DialogSpec::new(DialogIcon::Question, title, message)
        .with_button(DialogButton::new(cancel_label, CONFIRM_NO, ButtonRole::Neutral))
        .with_button(DialogButton::new(confirm_label, CONFIRM_YES, role))
        .dismissible(false))
}

/// Constrói o [`WindowSpec`] a partir do pedido `open_window(opts)` do prelúdio.
/// A fonte é `file` (caminho de template) ou `component` (nome já registrado no
/// motor de origem, resolvido para o arquivo em `run_on_owner`). `title`,
/// `width`/`height` e `resizable` são opcionais.
fn build_window_spec(lua: &Lua, req: &Table) -> mlua::Result<crate::component::WindowSpec> {
    use crate::component::WindowSpec;
    let mut spec = match (
        req.get::<Option<String>>("file")?,
        req.get::<Option<String>>("component")?,
    ) {
        (Some(file), _) => WindowSpec::file(file),
        (None, Some(name)) => WindowSpec::named(name),
        (None, None) => {
            return Err(mlua::Error::RuntimeError(
                "open_window: informe `file` (caminho) ou `component` (nome registrado)".into(),
            ));
        }
    };
    if let Some(title) = req.get::<Option<String>>("title")? {
        spec = spec.title(title);
    }
    if let (Some(w), Some(h)) = (
        req.get::<Option<f32>>("width")?,
        req.get::<Option<f32>>("height")?,
    ) {
        spec = spec.size(w, h);
    }
    if let Some(resizable) = req.get::<Option<bool>>("resizable")? {
        spec = spec.resizable(resizable);
    }
    // `data = { chave = valor, ... }`: semeia o contexto da nova janela. Cada
    // valor é convertido em string pela mesma regra de `ctx.foo = ...` (tabelas
    // viram JSON); `nil` é ignorado.
    if let Some(data) = req.get::<Option<Table>>("data")? {
        for pair in data.pairs::<String, Value>() {
            let (key, value) = pair?;
            if let Some(v) = luau_value_to_string(lua, &value) {
                spec = spec.with_data(key, v);
            }
        }
    }
    Ok(spec)
}

/// Constrói o [`ToastSpec`] a partir do pedido `toast(opts)` do prelúdio.
/// `kind` = "success"/"warning"/"error"/"info" (default info); `title` opcional.
fn build_toast(req: &Table) -> mlua::Result<crate::toasts::ToastSpec> {
    use crate::toasts::{ToastKind, ToastSpec};
    let message: String = req.get::<Option<String>>("message")?.unwrap_or_default();
    let kind = match req.get::<Option<String>>("kind")?.as_deref() {
        Some("success") => ToastKind::Success,
        Some("warning") => ToastKind::Warning,
        Some("error") => ToastKind::Error,
        _ => ToastKind::Info,
    };
    let mut spec = ToastSpec::new(kind, message);
    if let Some(title) = req.get::<Option<String>>("title")? {
        spec = spec.with_title(title);
    }
    Ok(spec)
}

/// Constrói uma [`NotificationSpec`] a partir da tabela pedida por `notify(opts)`
/// na camada Luau: `{ title?, body?, app_name?, icon? }` — todos opcionais (o
/// prelúdio normaliza uma string única para `body`).
fn build_notification(req: &Table) -> mlua::Result<crate::component::NotificationSpec> {
    Ok(crate::component::NotificationSpec {
        title: req.get::<Option<String>>("title")?.unwrap_or_default(),
        body: req.get::<Option<String>>("body")?.unwrap_or_default(),
        app_name: req.get::<Option<String>>("app_name")?,
        icon: req.get::<Option<String>>("icon")?,
    })
}

/// Converte um [`Value`] Luau na string que o contexto do motor guarda. Números
/// inteiros e floats de valor inteiro viram `"3"` (não `"3.0"`); `nil` devolve
/// `None` para não sobrescrever chaves com valor vazio à toa. Uma **tabela**
/// é serializada via `json.encode` por baixo (o mesmo caminho que `json`
/// expõe ao script) — `ctx.foo = {1,2,3}` grava `"[1,2,3]"` em vez de
/// desaparecer; uma tabela com função/thread/userdata dentro ainda falha
/// (devolve `None`, e quem chama loga o motivo — ver [`LuauComponent::sync_from_luau`]).
fn luau_value_to_string(lua: &Lua, v: &Value) -> Option<String> {
    match v {
        Value::Nil => None,
        Value::Boolean(b) => Some(b.to_string()),
        Value::Integer(i) => Some(i.to_string()),
        Value::Number(n) => {
            if n.fract() == 0.0 && n.is_finite() {
                Some((*n as i64).to_string())
            } else {
                Some(n.to_string())
            }
        }
        Value::String(s) => Some(s.to_string_lossy()),
        Value::Table(_) => {
            let json: serde_json::Value = lua.from_value(v.clone()).ok()?;
            serde_json::to_string(&json).ok()
        }
        _ => None,
    }
}

/// Chave (no registry do interpretador) da tabela que cacheia os módulos já
/// carregados por `require` — o equivalente ao `package.loaded` do Luau padrão,
/// mas privado ao motor.
const LOADED_KEY: &str = "glacier.loaded";

/// Diretórios onde `require` procura módulos, em ordem de prioridade, a partir
/// de `base_file` — o **script externo** (`<script src>`) quando há um, senão o
/// próprio template (script inline):
///
/// 1. o diretório de `base_file`;
/// 2. um subdiretório `lib/` dele (convenção para código compartilhado);
/// 3. cada caminho em `GLACIER_LUAU_PATH` (separados por `:`), para bibliotecas
///    fora da árvore do template.
///
/// Ancorar no diretório do *script* (e não no do template) é o que permite
/// separar `views/` de `views/scripts/` e ainda `require("net/api")` relativo ao
/// script.
fn module_roots(base_file: &Path) -> Vec<PathBuf> {
    let base = base_file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut roots = vec![base.clone(), base.join("lib")];
    if let Ok(extra) = std::env::var("GLACIER_LUAU_PATH") {
        roots.extend(
            extra
                .split(':')
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
        );
    }
    roots
}

/// Normaliza um `modname` de `require(...)` para uma string de caminho
/// relativo. Aceita ponto (`"net.http_client"`) OU barra (`"net/http_client"`)
/// como separador de pacote — como sempre — e agora também um ou mais
/// prefixos `./`/`../` de navegação explícita (estilo Node.js/Lua padrão), que
/// NÃO sofrem a conversão ponto→barra (só o restante da string sofre). Nenhum
/// nome de pacote legítimo começa com `.`, então os dois estilos nunca colidem.
fn normalize_modname(modname: &str) -> String {
    let mut prefix = String::new();
    let mut rest = modname;
    loop {
        if let Some(r) = rest.strip_prefix("../") {
            prefix.push_str("../");
            rest = r;
        } else if let Some(r) = rest.strip_prefix("./") {
            rest = r;
        } else {
            break;
        }
    }
    format!("{prefix}{}", rest.replace('.', "/"))
}

/// Resolve o nome de módulo `a.b.c` (ou `a/b/c`, opcionalmente prefixado por
/// `./`/`../`) para um arquivo `.luau`, testando `a/b/c.luau` e depois
/// `a/b/c/init.luau` em cada raiz, na ordem.
fn resolve_module(modname: &str, roots: &[PathBuf], assets: &dyn AssetSource) -> Option<PathBuf> {
    let rel = normalize_modname(modname);
    for ext in &["luau", "lua"] {
        for root in roots {
            let file = normalize_key(&root.join(format!("{rel}.{ext}")));
            if assets.exists(&file) {
                return Some(PathBuf::from(file));
            }
            let init = normalize_key(&root.join(&rel).join(format!("init.{ext}")));
            if assets.exists(&init) {
                return Some(PathBuf::from(init));
            }
        }
    }

    None
}

/// Colapsa `.`/`..` **lexicalmente** e unifica separadores em `/`, produzindo
/// uma chave estável de identidade de módulo sem tocar o filesystem.
///
/// Antes o cache de `require` usava [`Path::canonicalize`], que exige o arquivo
/// existir no disco; com uma fonte de assets embutida não há disco, então a
/// identidade tem de ser derivada só do texto do caminho. Preserva uma `/`
/// inicial (caminho absoluto vindo de `GLACIER_LUAU_PATH`).
fn normalize_key(path: &Path) -> String {
    use std::path::Component;
    let mut absolute = false;
    let mut parts: Vec<String> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::RootDir => absolute = true,
            Component::Prefix(p) => parts.push(p.as_os_str().to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(parts.last().map(String::as_str), Some(s) if s != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..".into());
                }
            }
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
        }
    }
    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

/// Instala um `require` próprio no interpretador, com resolução **relativa ao
/// arquivo que chama `require`** (como Node.js/Lua padrão) — não ao diretório
/// de trabalho nem, para módulos aninhados, ao script de entrada. Isso permite
/// organizar módulos em pacotes (subpastas que se referenciam entre si por
/// nome nu, como irmãos) e navegação explícita para fora do pacote atual com
/// `./`/`../`, exatamente como um type-checker Luau externo (ex.: `luau-lsp`)
/// já resolve — então os dois concordam sem gambiarra de estrutura de arquivo.
///
/// Regras de resolução, para cada `require(modname)`:
/// - **Prefixo `./`/`../` explícito**: busca **só** relativo ao diretório do
///   arquivo chamador. Sem fallback — igual a um import relativo em qualquer
///   outra linguagem, que não existindo é erro, não procura em outro lugar.
///   Erro se chamado de um contexto sem arquivo de origem (o script de nível
///   superior do componente, cujo chunk nunca tem nome `@arquivo` — ver
///   [`LuauComponent::build`]).
/// - **Nome nu** (sem prefixo): tenta primeiro o diretório do chamador (irmão
///   no mesmo pacote); se não achar, cai nas `roots` fixas de sempre (ver
///   [`module_roots`]: diretório do script de entrada, `lib/`,
///   `GLACIER_LUAU_PATH`) — mantém bibliotecas "globais" alcançáveis de
///   qualquer módulo aninhado, e é o único caminho disponível para requires
///   feitos direto do script de nível superior (que nunca tem "diretório do
///   chamador" próprio).
///
/// Como o módulo roda no **mesmo** interpretador do componente, ele enxerga o
/// prelúdio (`fetch`) e as globais — um client de rede importado pode chamar
/// `fetch` e suspender a corrotina da ação como qualquer código inline. Cada
/// módulo é carregado uma vez — o cache é por **caminho de arquivo resolvido**
/// (canonicalizado), não pela string pedida: a mesma string (ex.: `"types"`)
/// pode resolver a arquivos diferentes dependendo de quem chama, então a
/// identidade do cache tem que ser pelo arquivo, não pelo texto. O valor do
/// módulo é o que seu arquivo `return`a (uma tabela, por convenção); um módulo
/// sem `return` é cacheado como `true`.
fn install_module_system(
    luau: &Lua,
    roots: Vec<PathBuf>,
    assets: Arc<dyn AssetSource>,
) -> mlua::Result<()> {
    let cache = luau.create_table()?;
    luau.set_named_registry_value(LOADED_KEY, cache)?;

    let require = luau.create_function(move |luau, modname: String| {
        let is_explicit_relative = modname.starts_with("./") || modname.starts_with("../");

        // Diretório do chunk que está chamando require() agora (nível 1 acima
        // desta função nativa). `None` para o script de nível superior (seu
        // chunk se chama `<script:nome>`, sem prefixo `@`) — nesse caso só as
        // roots fixas se aplicam.
        let caller_dir: Option<PathBuf> = luau
            .inspect_stack(1, |dbg| {
                dbg.source()
                    .source
                    .as_ref()
                    .and_then(|s| s.strip_prefix('@').map(PathBuf::from))
            })
            .flatten()
            .and_then(|p| p.parent().map(Path::to_path_buf));

        let mut search: Vec<PathBuf> = Vec::new();
        if let Some(dir) = &caller_dir {
            search.push(dir.clone());
        }
        if is_explicit_relative {
            if caller_dir.is_none() {
                return Err(mlua::Error::runtime(format!(
                    "require('{modname}'): caminho relativo usado fora de um módulo com \
                     arquivo de origem (ex.: direto do script de nível superior)"
                )));
            }
        } else {
            search.extend(roots.iter().cloned());
        }

        let path = resolve_module(&modname, &search, assets.as_ref()).ok_or_else(|| {
            let procurados = search
                .iter()
                .map(|r| r.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            mlua::Error::runtime(format!(
                "módulo Luau '{modname}' não encontrado (procurado como \
                 '{rel}.luau' e '{rel}/init.luau' em: {procurados})",
                rel = normalize_modname(&modname),
            ))
        })?;

        // Cache pela chave lógica resolvida (já normalizada por
        // `resolve_module`), não pela string pedida — ver docstring da função.
        // O `canonicalize` de antes não serve numa fonte embutida (sem disco).
        let cache_key = path.to_string_lossy().into_owned();

        let cache: Table = luau.named_registry_value(LOADED_KEY)?;
        match cache.get::<Value>(cache_key.as_str())? {
            Value::Nil => {}
            cached => return Ok(cached),
        }

        let src = assets.read_to_string(&cache_key).map_err(|e| {
            mlua::Error::runtime(format!(
                "falha ao ler módulo Luau '{modname}' ({}): {e}",
                path.display()
            ))
        })?;

        let value: Value = luau
            .load(src.as_ref())
            .set_name(format!("@{}", path.display()))
            .eval()?;
        // Módulo sem `return` explícito vira `true`, como no Luau padrão, para
        // não recarregar a cada chamada.
        let value = match value {
            Value::Nil => Value::Boolean(true),
            v => v,
        };
        cache.set(cache_key.as_str(), value.clone())?;
        Ok(value)
    })?;

    luau.globals().set("require", require)?;
    Ok(())
}

/// Instala o global `json` com `json.encode(value)` e `json.decode(str)`,
/// ponte para o `serde_json` via [`LuaSerdeExt`] do `mlua`. É o que permite ao
/// `<script>` consumir a resposta de um `fetch` (`json.decode(res.body)`) e
/// montar payloads/strings JSON que os templates iteram
/// (`ctx.lista = json.encode(t)`), sem um parser em Lua puro.
///
/// Mapeamento:
/// - `decode`: string JSON → [`serde_json::Value`] → tabela Luau (objetos viram
///   tabelas por chave; arrays, tabelas 1-indexadas; `null` vira `nil`).
/// - `encode`: valor Luau → [`serde_json::Value`] → string JSON. Uma tabela com
///   chaves `1..n` sequenciais vira array JSON; caso contrário, objeto. Uma
///   tabela **vazia** é ambígua e serializa como objeto `{}` — quem precisa de
///   `[]` para uma lista vazia deve tratar esse caso no próprio script.
fn install_json(luau: &Lua) -> mlua::Result<()> {
    let json = luau.create_table()?;

    let decode = luau.create_function(|luau, s: String| {
        let v: serde_json::Value = serde_json::from_str(&s)
            .map_err(|e| mlua::Error::runtime(format!("json.decode: {e}")))?;
        // `null` → `nil` (não o sentinel userdata `null` do mlua): campos
        // ausentes/Option None viram `nil` em Lua, para os checks `x ~= nil` /
        // `x and …` funcionarem. Sem isto, `json.decode('{"d":null}').d` seria um
        // userdata (truthy!) e quebraria (ex.: `string.gsub(d, …)` → "got
        // userdata"). `set_array_metatable` fica ligado (default) para arrays
        // seguirem marcados (round-trip / `json.array`).
        let opts = mlua::SerializeOptions::new()
            .serialize_none_to_null(false)
            .serialize_unit_to_null(false);
        luau.to_value_with(&v, opts)
    })?;

    let encode = luau.create_function(|luau, v: Value| {
        let json: serde_json::Value = luau
            .from_value(v)
            .map_err(|e| mlua::Error::runtime(format!("json.encode: {e}")))?;
        serde_json::to_string(&json).map_err(|e| mlua::Error::runtime(format!("json.encode: {e}")))
    })?;

    // `json.array(t)` marca `t` como ARRAY para o encode, resolvendo a ambiguidade
    // da tabela vazia: `json.encode({})` produz `{}` (objeto), mas
    // `json.encode(json.array({}))` produz `[]`. Necessário ao (re)encodar structs
    // com campos `Vec` que podem ficar vazios (ex.: reencodar um spec editado
    // cujo `watch_paths`/`domains` esvaziou) — sem isso o servidor recusaria o
    // `[]` esperado ao ver um `{}`. Tabelas vindas de `json.decode('[]')` já vêm
    // marcadas; isto é para arrays CRIADOS no Luau. Sem efeito em tabelas com
    // itens (já detectadas como array). Devolve a própria tabela (encadeável).
    let array = luau.create_function(|luau, t: Table| {
        t.set_metatable(Some(luau.array_metatable()))?;
        Ok(t)
    })?;

    json.set("decode", decode)?;
    json.set("encode", encode)?;
    json.set("array", array)?;
    luau.globals().set("json", json)?;
    Ok(())
}

/// Caminho do arquivo de persistência de um componente: um `.json` por
/// componente sob `.glacier-storage/`, ao lado do arquivo que ancora
/// `require` (`module_base`, ver [`module_roots`]) — mesma convenção de
/// vizinhança que `lib/`. `name` é sanitizado (só `[A-Za-z0-9_-]`, resto vira
/// `_`) para nunca produzir um caminho fora do diretório esperado.
/// Raiz de gravação do global `storage`, definida pelo app hospedeiro (ver
/// [`set_storage_root`]). Quando presente, todos os motores do processo gravam
/// seus arquivos de `storage` sob esta pasta em vez de relativo ao diretório do
/// script — necessário quando os assets moram num diretório read-only (ex.: um
/// app empacotado que roda de `/usr/share`), onde o local legado não é
/// gravável. `OnceLock`: é config global do processo, semeada uma vez na
/// inicialização e compartilhada por todas as janelas.
static STORAGE_ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Define a raiz de gravação do global `storage` (ver [`STORAGE_ROOT`]). Chame
/// uma única vez, antes de subir os motores (o [`crate::GlacierDaemon`] a chama
/// a partir de `storage_dir`). Sem isto, `storage` mantém o comportamento
/// legado: grava em `.glacier-storage/` relativo ao diretório do script.
pub fn set_storage_root(path: PathBuf) {
    let _ = STORAGE_ROOT.set(path);
}

/// Resolve o arquivo de `storage` de um componente. Quando `root` é `Some`
/// (raiz definida pelo app via [`set_storage_root`]), grava sob ela; senão,
/// mantém o comportamento legado: relativo ao diretório do script
/// (`module_base`). Recebe a raiz por parâmetro (resolvida do global no
/// call-site) em vez de ler o global aqui, para permanecer uma função pura,
/// testável sem tocar no [`STORAGE_ROOT`] (que é global do processo e só pode
/// ser semeado uma vez).
fn storage_path(module_base: &Path, name: &str, root: Option<&Path>) -> PathBuf {
    let base = match root {
        Some(root) => root.to_path_buf(),
        None => module_base
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
    };
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    base.join(".glacier-storage").join(format!("{safe}.json"))
}

/// Lê o arquivo de persistência como um objeto JSON (vazio se ausente ou
/// corrompido — persistência é "best effort", não deve derrubar o script).
fn read_storage_file(path: &Path) -> serde_json::Map<String, serde_json::Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| match v {
            serde_json::Value::Object(m) => Some(m),
            _ => None,
        })
        .unwrap_or_default()
}

/// Grava o objeto JSON de volta no arquivo de persistência, criando o
/// diretório `.glacier-storage/` se necessário. Falhas de I/O são logadas,
/// não propagadas — `storage.set` não deveria poder derrubar um script.
fn write_storage_file(path: &Path, map: &serde_json::Map<String, serde_json::Value>) {
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        eprintln!(
            "[glacier-ui] storage: falha ao criar '{}': {}",
            parent.display(),
            e
        );
        return;
    }
    match serde_json::to_string_pretty(&serde_json::Value::Object(map.clone())) {
        Ok(s) => {
            if let Err(e) = std::fs::write(path, s) {
                eprintln!(
                    "[glacier-ui] storage: falha ao gravar '{}': {}",
                    path.display(),
                    e
                );
            }
        }
        Err(e) => eprintln!("[glacier-ui] storage: falha ao serializar: {}", e),
    }
}

/// Instala o global `storage` (`get`/`set`/`remove`), persistência local em
/// JSON análoga ao `localStorage` do browser — o que sobrevive a um restart
/// do processo (ao contrário de `ctx`, que é só memória). Cada chamada lê/
/// grava o arquivo inteiro (simples e correto para o volume de dados que um
/// `<script>` de UI guarda; não pensado para alta frequência/concorrência).
///
/// - `storage.get(key)`: devolve o valor guardado (qualquer tipo
///   JSON-serializável — string, número, booleano, tabela) ou `nil`.
/// - `storage.set(key, value)`: grava `value` sob `key`, sobrescrevendo.
/// - `storage.remove(key)`: apaga `key`, se existir.
fn install_storage(luau: &Lua, path: PathBuf) -> mlua::Result<()> {
    let storage = luau.create_table()?;

    let get_path = path.clone();
    let get = luau.create_function(move |luau, key: String| {
        let map = read_storage_file(&get_path);
        match map.get(&key) {
            Some(v) => luau.to_value(v),
            None => Ok(Value::Nil),
        }
    })?;

    let set_path = path.clone();
    let set = luau.create_function(move |luau, (key, value): (String, Value)| {
        let mut map = read_storage_file(&set_path);
        let json: serde_json::Value = luau
            .from_value(value)
            .map_err(|e| mlua::Error::runtime(format!("storage.set: {e}")))?;
        map.insert(key, json);
        write_storage_file(&set_path, &map);
        Ok(())
    })?;

    let remove_path = path.clone();
    let remove = luau.create_function(move |_, key: String| {
        let mut map = read_storage_file(&remove_path);
        map.remove(&key);
        write_storage_file(&remove_path, &map);
        Ok(())
    })?;

    storage.set("get", get)?;
    storage.set("set", set)?;
    storage.set("remove", remove)?;
    luau.globals().set("storage", storage)?;
    Ok(())
}

/// Instala o global `write_file(path, conteúdo)` — escrita de arquivo local,
/// contraparte do `fetch("file://…")` (leitura). Ao contrário do `storage`
/// (JSON chaveado num arquivo que o glacier gerencia), este grava o conteúdo
/// literal no caminho dado pelo chamador, criando o diretório pai se preciso.
///
/// Síncrono (como o `storage`): escrita local é rápida e não justifica suspender
/// a corrotina. Nunca derruba o script — falha de I/O vira valor de retorno:
///
/// - sucesso: `write_file(path, txt)` → `true`
/// - falha:   `write_file(path, txt)` → `false, "<mensagem>"`
fn install_write_file(luau: &Lua) -> mlua::Result<()> {
    let write_file = luau.create_function(|_, (path, contents): (String, String)| {
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            return Ok((false, Some(e.to_string())));
        }
        match std::fs::write(&path, contents) {
            Ok(()) => Ok((true, None)),
            Err(e) => Ok((false, Some(e.to_string()))),
        }
    })?;
    luau.globals().set("write_file", write_file)?;
    Ok(())
}

/// Se o template tem um bloco `<script>` (inline ou apontando para um `.luau`
/// externo via `src`/`from`) — ou seja, se ele traz comportamento Luau. O motor
/// usa isto para decidir, ao registrar um componente por arquivo, se liga um
/// [`LuauComponent`] (há script) ou o mantém só-UI (não há).
pub(crate) fn has_script(markup: &str) -> bool {
    extract_script_src(markup).is_some() || extract_script(markup).is_some()
}

/// Resolve o corpo Luau de um template: se o `<script>` referencia um arquivo
/// externo via `src="..."` (ou `from="..."`), lê esse arquivo (caminho relativo
/// ao diretório do `template_path`); senão, usa o corpo inline do bloco.
/// Devolve `(conteúdo, Option<caminho do script externo>)`. Para um
/// `<script src>` externo, o caminho é resolvido relativo ao diretório do
/// template e devolvido para ancorar a resolução de `require` (ver
/// [`LuauComponent::from_file`]). Inline → `(corpo, None)`.
fn resolve_script(
    markup: &str,
    template_path: &str,
    assets: &dyn AssetSource,
) -> std::result::Result<(String, Option<PathBuf>), String> {
    if let Some(src) = extract_script_src(markup) {
        let base = std::path::Path::new(template_path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let luau_path = PathBuf::from(normalize_key(&base.join(&src)));
        let content = assets
            .read_to_string(&luau_path.to_string_lossy())
            .map_err(|e| {
                format!(
                    "Falha ao ler script Luau externo '{}': {}",
                    luau_path.display(),
                    e
                )
            })?
            .into_owned();
        return Ok((content, Some(luau_path)));
    }
    Ok((extract_script(markup).unwrap_or_default(), None))
}

/// Lê o atributo `src`/`from` da tag de abertura `<script ...>`, se houver — o
/// caminho de um arquivo `.luau` externo.
fn extract_script_src(markup: &str) -> Option<String> {
    let lower = markup.to_ascii_lowercase();
    let open = lower.find("<script")?;
    // Só o texto da tag de abertura (até o primeiro `>`).
    let gt = lower[open..].find('>')? + open;
    let tag = &markup[open..gt];
    let re = regex::Regex::new(r#"(?i)\b(?:src|from)\s*=\s*["']([^"']+)["']"#).ok()?;
    re.captures(tag)
        .map(|c| c.get(1).map_or(String::new(), |m| m.as_str().to_string()))
        .filter(|s| !s.is_empty())
}

/// Extrai o corpo de um bloco `<script>...</script>` de um template XML.
/// Espelha a lógica de remoção do parser, mas devolve o conteúdo em vez de
/// descartá-lo.
fn extract_script(markup: &str) -> Option<String> {
    let lower = markup.to_ascii_lowercase();
    let open = lower.find("<script")?;
    let gt = lower[open..].find('>')? + open + 1;
    let close = lower[gt..].find("</script>")? + gt;
    Some(markup[gt..close].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Roda `func`/`value` contra um mapa de contexto e devolve o mapa mutado,
    /// exercitando o mesmo caminho de `update`.
    fn drive(
        comp: &LuauComponent,
        func: &str,
        value: Option<&str>,
        mut data: HashMap<String, String>,
    ) -> HashMap<String, String> {
        let mut ctx = Context::new(&mut data);
        comp.run(func, value, &mut ctx);
        data
    }

    #[test]
    fn incrementa_lendo_e_escrevendo_o_contexto() {
        let comp = LuauComponent::from_source(
            "function incrementar() ctx.contador = ctx.contador + 1 end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        data.insert("contador".into(), "0".into());
        let data = drive(&comp, "incrementar", None, data);
        // Coerção de string numérica + volta a inteiro (não "1.0").
        assert_eq!(data.get("contador").map(String::as_str), Some("1"));
    }

    #[test]
    fn onchange_recebe_o_valor() {
        let comp = LuauComponent::from_source("function set_nome(v) ctx.nome = v end", "t.gv", "c")
            .unwrap();
        let data = drive(&comp, "set_nome", Some("Ana"), HashMap::new());
        assert_eq!(data.get("nome").map(String::as_str), Some("Ana"));
    }

    #[test]
    fn atribuir_nil_remove_a_chave_no_contexto() {
        let comp = LuauComponent::from_source("function limpar() ctx.temp = nil end", "t.gv", "c")
            .unwrap();
        let mut data = HashMap::new();
        data.insert("temp".into(), "algo".into());
        data.insert("manter".into(), "ok".into());
        let data = drive(&comp, "limpar", None, data);
        assert_eq!(
            data.get("temp"),
            None,
            "ctx.temp = nil deveria remover a chave"
        );
        // Chaves não tocadas pelo script permanecem.
        assert_eq!(data.get("manter").map(String::as_str), Some("ok"));
    }

    #[test]
    fn acao_sem_funcao_e_ignorada() {
        let comp = LuauComponent::from_source("function a() end", "t.gv", "c").unwrap();
        let mut data = HashMap::new();
        data.insert("x".into(), "keep".into());
        let data = drive(&comp, "inexistente", None, data);
        assert_eq!(data.get("x").map(String::as_str), Some("keep"));
    }

    #[test]
    fn acao_com_sufixo_passa_o_sufixo_como_argumento() {
        // `open_service:<id>` deve chamar open_service("abc") quando não existe
        // uma função com o nome exato "open_service:abc".
        let comp = LuauComponent::from_source(
            "function open_service(id) ctx.aberto = id end",
            "t.gv",
            "c",
        )
        .unwrap();
        let data = drive(&comp, "open_service:abc", None, HashMap::new());
        assert_eq!(data.get("aberto").map(String::as_str), Some("abc"));
    }

    #[test]
    fn acao_com_sufixo_e_value_passa_ambos() {
        // `field:<chave>` num onChange chama field(chave, texto): sufixo 1º, o
        // valor do input em seguida.
        let comp =
            LuauComponent::from_source("function field(k, v) ctx[k] = v end", "t.gv", "c").unwrap();
        let data = drive(&comp, "field:nome", Some("Ana"), HashMap::new());
        assert_eq!(data.get("nome").map(String::as_str), Some("Ana"));
    }

    #[test]
    fn nome_exato_tem_precedencia_sobre_o_split() {
        // Se existe função com o nome literal (sem interpretar ':'), ela ganha.
        // (Nomes Lua não contêm ':', então na prática o exato é sempre sem ':'.)
        let comp = LuauComponent::from_source(
            "function salvar(v) ctx.via = 'exato' end \
             function salvar_x() ctx.via = 'split' end",
            "t.gv",
            "c",
        )
        .unwrap();
        let data = drive(&comp, "salvar", None, HashMap::new());
        assert_eq!(data.get("via").map(String::as_str), Some("exato"));
    }

    #[test]
    fn toast_empilha_no_contexto() {
        let comp = LuauComponent::from_source(
            "function go() toast({ message='falhou', kind='error', title='Erro' }) end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.run("go", None, &mut ctx);
        assert_eq!(ctx.toasts.len(), 1);
        assert_eq!(ctx.toasts[0].message, "falhou");
        assert_eq!(ctx.toasts[0].kind, crate::toasts::ToastKind::Error);
    }

    #[test]
    fn toast_aceita_string_curta() {
        let comp =
            LuauComponent::from_source("function go() toast('oi') end", "t.gv", "c").unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.run("go", None, &mut ctx);
        assert_eq!(ctx.toasts.len(), 1);
        assert_eq!(ctx.toasts[0].message, "oi");
    }

    #[test]
    fn confirm_abre_dialogo_resumivel_e_suspende() {
        let comp = LuauComponent::from_source(
            "function go() confirm({ title='T', message='M', confirm_label='Sim', \
             destructive=true }) end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.run("go", None, &mut ctx);
        match &ctx.dialog {
            Some(crate::component::DialogAction::ShowResumable(spec, _id)) => {
                assert_eq!(spec.buttons.len(), 2, "cancelar + confirmar");
                assert_eq!(spec.buttons[1].action, crate::dialogs::CONFIRM_YES);
                assert_eq!(
                    spec.buttons[1].role,
                    crate::dialogs::ButtonRole::Destructive
                );
                assert_eq!(spec.buttons[0].action, crate::dialogs::CONFIRM_NO);
            }
            _ => panic!("esperava um diálogo ShowResumable"),
        }
    }

    #[test]
    fn confirm_retoma_com_booleano_da_escolha() {
        // Confirmar (`true`) roda o ramo então; cancelar (`false`) roda o senão.
        // Exercita o caminho `drive` → suspende no `confirm` → `resume_dialog`.
        for (confirmed, esperado) in [(true, "sim"), (false, "nao")] {
            let mut comp = LuauComponent::from_source(
                "function go()\n\
                 if confirm({ title='T', message='M' }) then ctx.r = 'sim' \
                 else ctx.r = 'nao' end\n\
                 end",
                "t.gv",
                "c",
            )
            .unwrap();
            let mut data = HashMap::new();
            let id = {
                let mut ctx = Context::new(&mut data);
                comp.run("go", None, &mut ctx);
                match ctx.dialog {
                    Some(crate::component::DialogAction::ShowResumable(_, id)) => id,
                    _ => panic!("esperava suspender num diálogo resumível"),
                }
            };
            // Ainda não decidiu: o ramo não rodou.
            assert_eq!(data.get("r"), None, "não deve resolver antes da escolha");
            {
                let mut ctx = Context::new(&mut data);
                comp.resume_dialog(id, confirmed, &mut ctx);
            }
            assert_eq!(data.get("r").map(String::as_str), Some(esperado));
        }
    }

    #[test]
    fn json_decode_null_vira_nil_nao_userdata() {
        // Campo `null` deve virar `nil` (ausente), não o sentinel userdata do
        // mlua — senão `x ~= nil`/`x and …` falham e ops de string quebram
        // ("string expected, got userdata").
        let comp = LuauComponent::from_source(
            "function go()\n\
               local t = json.decode('{\"d\":null,\"n\":\"x\"}')\n\
               ctx.d_nil = tostring(t.d == nil)\n\
               ctx.d_type = type(t.d)\n\
               -- não deve erro: gsub num campo null-que-virou-nil guardado com fallback\n\
               ctx.safe = (t.d or 'vazio')\n\
             end",
            "t.gv",
            "c",
        )
        .unwrap();
        let d = drive(&comp, "go", None, HashMap::new());
        assert_eq!(d.get("d_nil").map(String::as_str), Some("true"));
        assert_eq!(d.get("d_type").map(String::as_str), Some("nil"));
        assert_eq!(d.get("safe").map(String::as_str), Some("vazio"));
    }

    #[test]
    fn json_array_forca_colchetes_em_tabela_vazia() {
        let comp = LuauComponent::from_source(
            "function go()\n\
               ctx.obj = json.encode({})\n\
               ctx.arr = json.encode(json.array({}))\n\
               ctx.nested = json.encode({ ws = json.array({}), name = 'x' })\n\
             end",
            "t.gv",
            "c",
        )
        .unwrap();
        let d = drive(&comp, "go", None, HashMap::new());
        assert_eq!(d.get("obj").map(String::as_str), Some("{}"));
        assert_eq!(d.get("arr").map(String::as_str), Some("[]"));
        assert_eq!(
            d.get("nested").map(String::as_str),
            Some(r#"{"name":"x","ws":[]}"#)
        );
    }

    #[test]
    fn form_control_sem_handler_escreve_no_contexto() {
        // Um `formControl="url"` (onChange implícito = "url") sem função `url`
        // no script deve gravar ctx.url com o texto digitado — o loop que o
        // Form do Rust fechava, agora nativo para componentes Luau.
        let comp = LuauComponent::from_source("function init() end", "t.gv", "c").unwrap();
        let data = drive(&comp, "url", Some("https://x.tech"), HashMap::new());
        assert_eq!(data.get("url").map(String::as_str), Some("https://x.tech"));
    }

    #[test]
    fn acao_simples_sem_value_e_sem_funcao_e_ignorada() {
        // Sem função e sem value, nada acontece (não cria chave espúria).
        let comp = LuauComponent::from_source("function a() end", "t.gv", "c").unwrap();
        let data = drive(&comp, "inexistente", None, HashMap::new());
        assert_eq!(data.get("inexistente"), None);
    }

    #[test]
    fn init_semea_default() {
        let comp = LuauComponent::from_source(
            "function init() ctx.contador = ctx.contador or 5 end",
            "t.gv",
            "c",
        )
        .unwrap();
        let data = drive(&comp, "init", None, HashMap::new());
        assert_eq!(data.get("contador").map(String::as_str), Some("5"));
    }

    #[test]
    fn json_decode_le_campos_de_um_objeto() {
        let comp = LuauComponent::from_source(
            "function go() local t = json.decode(ctx.raw) ctx.nome = t.nome ctx.n = t.n end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        data.insert("raw".into(), r#"{"nome":"web","n":3}"#.into());
        let data = drive(&comp, "go", None, data);
        assert_eq!(data.get("nome").map(String::as_str), Some("web"));
        // Número inteiro do JSON vira inteiro Luau → "3" (não "3.0").
        assert_eq!(data.get("n").map(String::as_str), Some("3"));
    }

    #[test]
    fn json_encode_de_array_preserva_ordem() {
        let comp = LuauComponent::from_source(
            "function go() ctx.out = json.encode({ 'a', 'b', 'c' }) end",
            "t.gv",
            "c",
        )
        .unwrap();
        let data = drive(&comp, "go", None, HashMap::new());
        // Tabela 1-indexada sequencial → array JSON, na ordem.
        assert_eq!(
            data.get("out").map(String::as_str),
            Some(r#"["a","b","c"]"#)
        );
    }

    #[test]
    fn json_roundtrip_array_de_objetos_reencaixa_a_forma() {
        // Espelha o uso real: decodificar a lista crua de uma API e reencodar
        // só os campos que o template itera.
        let comp = LuauComponent::from_source(
            "function build()\n\
               local svcs = json.decode(ctx.raw)\n\
               local out = {}\n\
               for i, s in ipairs(svcs) do out[i] = { name = s.name, up = s.running } end\n\
               ctx.services = json.encode(out)\n\
             end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        data.insert(
            "raw".into(),
            r#"[{"name":"web","running":true},{"name":"db","running":false}]"#.into(),
        );
        let data = drive(&comp, "build", None, data);
        // Ordem de chaves de objeto não é garantida; valida a estrutura.
        let out: serde_json::Value =
            serde_json::from_str(data.get("services").expect("services")).unwrap();
        assert_eq!(out[0]["name"], "web");
        assert_eq!(out[0]["up"], true);
        assert_eq!(out[1]["name"], "db");
        assert_eq!(out[1]["up"], false);
    }

    #[test]
    fn fetch_suspende_a_corrotina_e_retoma_com_a_resposta() {
        let comp = LuauComponent::from_source(
            r#"
            function carregar()
                local res = fetch("http://exemplo/api", { method = "POST", body = "q" })
                if res.ok then ctx.dados = res.body else ctx.erro = res.error end
            end
            "#,
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();

        // 1) roda a ação: `fetch` cede, a corrotina suspende e um PendingFetch aparece.
        let id;
        {
            let mut ctx = Context::new(&mut data);
            comp.run("carregar", None, &mut ctx);
            assert_eq!(ctx.fetches.len(), 1, "deveria ter suspendido num fetch");
            assert_eq!(ctx.fetches[0].url, "http://exemplo/api");
            assert_eq!(ctx.fetches[0].method, "POST");
            assert_eq!(ctx.fetches[0].body.as_deref(), Some("q"));
            id = ctx.fetches[0].id;
        }

        // 2) o motor entrega a resposta: a corrotina retoma no ponto do fetch.
        {
            let mut ctx = Context::new(&mut data);
            let res = FetchResult {
                ok: true,
                status: 200,
                body: "OLA".into(),
                error: String::new(),
            };
            comp.resume_inner(id, &res, &mut ctx).unwrap();
        }
        assert_eq!(data.get("dados").map(String::as_str), Some("OLA"));
        assert_eq!(data.get("erro"), None);
    }

    #[test]
    fn sse_registra_stream_sem_suspender_e_handler_recebe_mensagens() {
        let mut comp = LuauComponent::from_source(
            "function init() sse('http://ex/stream', { on_message = 'on_ev', on_close = 'on_fim' }) end\n\
             function on_ev(d) ctx.ultima = d end\n\
             function on_fim() ctx.fim = 'sim' end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();

        // init abre o stream: um StreamRequest aparece e a corrotina NÃO fica
        // suspensa (sse não bloqueia como fetch).
        let id;
        {
            let mut ctx = Context::new(&mut data);
            comp.run("init", None, &mut ctx);
            assert_eq!(ctx.streams.len(), 1, "init deveria ter aberto 1 stream");
            assert_eq!(ctx.streams[0].kind, StreamKind::Sse);
            assert_eq!(ctx.streams[0].url, "http://ex/stream");
            assert!(ctx.fetches.is_empty(), "sse não usa o caminho de fetch");
            id = ctx.streams[0].id;
        }

        // Uma mensagem do stream chama on_message('on_ev') com o texto.
        {
            let mut ctx = Context::new(&mut data);
            comp.on_stream_event(id, StreamEventKind::Message, "oi", &mut ctx);
        }
        assert_eq!(data.get("ultima").map(String::as_str), Some("oi"));

        // Ao fechar, on_close roda e o registro do stream é descartado.
        {
            let mut ctx = Context::new(&mut data);
            comp.on_stream_event(id, StreamEventKind::Closed, "", &mut ctx);
        }
        assert_eq!(data.get("fim").map(String::as_str), Some("sim"));
        assert!(
            comp.streams.borrow().is_empty(),
            "Closed deveria limpar o registro"
        );
    }

    #[test]
    fn websocket_handle_envia_comando_de_saida() {
        let comp = LuauComponent::from_source(
            "function go()\n\
               local c = websocket('ws://ex', { on_message = 'on_ev' })\n\
               c:send('ola')\n\
             end\n\
             function on_ev(d) ctx.ultima = d end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.run("go", None, &mut ctx);
        // Abriu um WebSocket e enfileirou um `send` — sem suspender.
        assert_eq!(ctx.streams.len(), 1);
        assert_eq!(ctx.streams[0].kind, StreamKind::Ws);
        assert_eq!(
            ctx.stream_cmds.len(),
            1,
            "c:send deveria enfileirar 1 comando"
        );
        assert_eq!(ctx.stream_cmds[0].kind, StreamCommandKind::Send);
        assert_eq!(ctx.stream_cmds[0].text, "ola");
        // O comando referencia o mesmo id do stream aberto.
        assert_eq!(ctx.stream_cmds[0].id, ctx.streams[0].id);
    }

    #[test]
    fn detecta_src_externo() {
        assert_eq!(
            extract_script_src(r#"<script src="scripts/c.luau"></script>"#).as_deref(),
            Some("scripts/c.luau")
        );
        assert_eq!(
            extract_script_src(r#"<script from='a.luau' />"#).as_deref(),
            Some("a.luau")
        );
        // Sem src: inline, então None.
        assert_eq!(extract_script_src("<script> a </script>"), None);
    }

    #[test]
    fn carrega_luau_de_arquivo_externo_relativo_ao_template() {
        // Monta template + .luau num diretório temporário e confere que o `src`
        // é resolvido relativo ao diretório do template.
        let dir = std::env::temp_dir().join(format!("glacier_lua_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tpl = dir.join("t.gv");
        let luau = dir.join("beh.luau");
        std::fs::write(&luau, "function incrementar() ctx.n = ctx.n + 1 end").unwrap();
        std::fs::write(&tpl, r#"<Text/><script src="beh.luau"></script>"#).unwrap();

        let comp = LuauComponent::from_file(tpl.to_str().unwrap(), "c").unwrap();
        let mut data = HashMap::new();
        data.insert("n".into(), "41".into());
        let data = drive(&comp, "incrementar", None, data);
        assert_eq!(data.get("n").map(String::as_str), Some("42"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_file_grava_arquivo_local() {
        let dir = std::env::temp_dir().join(format!("glacier_writefile_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Grava num subdiretório inexistente para exercitar o create_dir_all.
        let alvo = dir.join("sub").join("saida.txt");
        let comp = LuauComponent::from_source(
            "function gravar()\n\
             local ok, err = write_file(ctx.path, 'conteúdo do luau')\n\
             ctx.ok = tostring(ok)\n\
             ctx.err = err or ''\n\
             end",
            dir.join("t.gv").to_str().unwrap(),
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        data.insert("path".into(), alvo.to_str().unwrap().to_string());
        let data = drive(&comp, "gravar", None, data);

        assert_eq!(data.get("ok").map(String::as_str), Some("true"));
        assert_eq!(data.get("err").map(String::as_str), Some(""));
        assert_eq!(
            std::fs::read_to_string(&alvo).unwrap(),
            "conteúdo do luau"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Cria um diretório temporário exclusivo do teste (isolado por nome), útil
    /// para montar árvores de módulos.
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("glacier_luau_{}_{}", std::process::id(), tag));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn require_carrega_modulo_relativo_ao_template() {
        let dir = temp_dir("require_rel");
        std::fs::create_dir_all(dir.join("util")).unwrap();
        // Biblioteca pura: sem rede, só lógica encapsulada.
        std::fs::write(
            dir.join("util").join("strings.luau"),
            "local M = {}\nfunction M.shout(s) return s:upper() .. '!' end\nreturn M\n",
        )
        .unwrap();
        let comp = LuauComponent::from_source(
            "local strings = require('util.strings')\n\
             function grita() ctx.msg = strings.shout(ctx.msg) end",
            dir.join("t.gv").to_str().unwrap(),
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        data.insert("msg".into(), "ola".into());
        let data = drive(&comp, "grita", None, data);
        assert_eq!(data.get("msg").map(String::as_str), Some("OLA!"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn require_cacheia_modulo_uma_vez() {
        let dir = temp_dir("require_cache");
        // O módulo incrementa um contador global A CADA carga; se `require`
        // cacheasse errado (recarregando), o contador subiria.
        std::fs::write(
            dir.join("once.luau"),
            "_G.__cargas = (_G.__cargas or 0) + 1\nreturn { n = _G.__cargas }\n",
        )
        .unwrap();
        let comp = LuauComponent::from_source(
            "local a = require('once')\nlocal b = require('once')\n\
             function checar() ctx.cargas = a.n ctx.mesmo = tostring(a == b) end",
            dir.join("t.gv").to_str().unwrap(),
            "c",
        )
        .unwrap();
        let data = drive(&comp, "checar", None, HashMap::new());
        assert_eq!(data.get("cargas").map(String::as_str), Some("1"));
        assert_eq!(data.get("mesmo").map(String::as_str), Some("true"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn modulo_importado_pode_chamar_fetch_e_suspender() {
        let dir = temp_dir("require_fetch");
        std::fs::create_dir_all(dir.join("net")).unwrap();
        // Client de rede reutilizável: encapsula base_url e usa `fetch` por baixo.
        std::fs::write(
            dir.join("net").join("client.luau"),
            "local Client = {}\nClient.__index = Client\n\
             function Client.new(base) return setmetatable({ base = base }, Client) end\n\
             function Client:get(p) return fetch(self.base .. p) end\n\
             return Client\n",
        )
        .unwrap();
        let comp = LuauComponent::from_source(
            "local http = require('net.client')\nlocal api = http.new('http://ex')\n\
             function carregar() local r = api:get('/x') if r.ok then ctx.dados = r.body end end",
            dir.join("t.gv").to_str().unwrap(),
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();

        // A chamada ao módulo suspende a corrotina num fetch — prova de que o
        // `fetch` do prelúdio funciona dentro do módulo importado.
        let id;
        {
            let mut ctx = Context::new(&mut data);
            comp.run("carregar", None, &mut ctx);
            assert_eq!(
                ctx.fetches.len(),
                1,
                "o módulo deveria ter suspendido num fetch"
            );
            assert_eq!(ctx.fetches[0].url, "http://ex/x");
            id = ctx.fetches[0].id;
        }
        {
            let mut ctx = Context::new(&mut data);
            let res = FetchResult {
                ok: true,
                status: 200,
                body: "PONG".into(),
                error: String::new(),
            };
            comp.resume_inner(id, &res, &mut ctx).unwrap();
        }
        assert_eq!(data.get("dados").map(String::as_str), Some("PONG"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn require_de_modulo_inexistente_falha_com_mensagem_clara() {
        let dir = temp_dir("require_missing");
        let comp = LuauComponent::from_source(
            "function usar() require('nao.existe') end",
            dir.join("t.gv").to_str().unwrap(),
            "c",
        )
        .unwrap();
        // Não deve derrubar o processo: o erro é logado e a ação vira no-op.
        let data = drive(&comp, "usar", None, HashMap::new());
        assert!(data.is_empty());
        // A resolução em si devolve None para um módulo ausente.
        assert!(resolve_module("nao.existe", &module_roots(&dir.join("t.gv")), &DiskAssets).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn require_de_script_externo_resolve_relativo_ao_script() {
        // Template em <dir>/app.gv com `<script src="scripts/app.luau">`; o
        // script faz `require("m")` que deve resolver <dir>/scripts/m.luau
        // (relativo ao SCRIPT, não ao template) — permite separar views/scripts.
        let dir = std::env::temp_dir().join(format!("glacier_ext_{}", std::process::id()));
        let scripts = dir.join("scripts");
        std::fs::create_dir_all(&scripts).unwrap();
        std::fs::write(
            dir.join("app.gv"),
            "<Column></Column>\n<script src=\"scripts/app.luau\"></script>",
        )
        .unwrap();
        std::fs::write(
            scripts.join("app.luau"),
            "local m = require(\"m\")\nfunction go() ctx.v = m.hi end",
        )
        .unwrap();
        std::fs::write(scripts.join("m.luau"), "return { hi = \"ok\" }").unwrap();

        let comp = LuauComponent::from_file(dir.join("app.gv").to_str().unwrap(), "app").unwrap();
        let data = drive(&comp, "go", None, HashMap::new());
        assert_eq!(data.get("v").map(String::as_str), Some("ok"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn require_bare_entre_irmaos_em_modulo_aninhado_resolve_pelo_proprio_diretorio() {
        // <dir>/app.gv (template) -> scripts/app.luau (topo) faz
        // require("pkg/a"); pkg/a.luau, por sua vez, faz require("b") (NOME
        // NU, sem prefixo) esperando achar pkg/b.luau — não <dir>/scripts/b.luau.
        // Antes do fix, a resolução era sempre pelas roots fixas (dir do
        // script de ENTRADA), então isto teria falhado: o bare "b" só existia
        // relativo ao pacote pkg/, não à raiz.
        let dir = std::env::temp_dir().join(format!("glacier_pkg_sib_{}", std::process::id()));
        let scripts = dir.join("scripts");
        let pkg = scripts.join("pkg");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(
            dir.join("app.gv"),
            "<Column></Column>\n<script src=\"scripts/app.luau\"></script>",
        )
        .unwrap();
        std::fs::write(
            scripts.join("app.luau"),
            "local A = require(\"pkg/a\")\nfunction go() ctx.v = A.hi end",
        )
        .unwrap();
        std::fs::write(
            pkg.join("a.luau"),
            "local B = require(\"b\")\nreturn { hi = B.msg }",
        )
        .unwrap();
        std::fs::write(pkg.join("b.luau"), "return { msg = \"irmao-ok\" }").unwrap();

        let comp = LuauComponent::from_file(dir.join("app.gv").to_str().unwrap(), "app").unwrap();
        let data = drive(&comp, "go", None, HashMap::new());
        assert_eq!(data.get("v").map(String::as_str), Some("irmao-ok"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn require_com_prefixo_dotdot_sobe_um_nivel() {
        // scripts/pkg/a.luau faz require("../shared") esperando achar
        // scripts/shared.luau (um nível acima do próprio pacote) — navegação
        // explícita estilo Node, sem depender do fallback de roots fixas.
        let dir = std::env::temp_dir().join(format!("glacier_dotdot_{}", std::process::id()));
        let scripts = dir.join("scripts");
        let pkg = scripts.join("pkg");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(
            dir.join("app.gv"),
            "<Column></Column>\n<script src=\"scripts/app.luau\"></script>",
        )
        .unwrap();
        std::fs::write(
            scripts.join("app.luau"),
            "local A = require(\"pkg/a\")\nfunction go() ctx.v = A.hi end",
        )
        .unwrap();
        std::fs::write(
            pkg.join("a.luau"),
            "local S = require(\"../shared\")\nreturn { hi = S.msg }",
        )
        .unwrap();
        std::fs::write(scripts.join("shared.luau"), "return { msg = \"subiu-ok\" }").unwrap();

        let comp = LuauComponent::from_file(dir.join("app.gv").to_str().unwrap(), "app").unwrap();
        let data = drive(&comp, "go", None, HashMap::new());
        assert_eq!(data.get("v").map(String::as_str), Some("subiu-ok"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn require_de_mesmo_nome_em_pacotes_diferentes_nao_colide_no_cache() {
        // pkg_a/x.luau e pkg_b/x.luau são arquivos DIFERENTES com o MESMO nome
        // "x". Dois módulos, um em cada pacote, fazem require("x") (bare,
        // irmão) — cada um deve receber o SEU, não um cacheado por engano por
        // causa da mesma string "x" (prova a mudança de chave do cache para
        // caminho resolvido, não string pedida).
        let dir = std::env::temp_dir().join(format!("glacier_cache_{}", std::process::id()));
        let scripts = dir.join("scripts");
        let pa = scripts.join("pkg_a");
        let pb = scripts.join("pkg_b");
        std::fs::create_dir_all(&pa).unwrap();
        std::fs::create_dir_all(&pb).unwrap();
        std::fs::write(
            dir.join("app.gv"),
            "<Column></Column>\n<script src=\"scripts/app.luau\"></script>",
        )
        .unwrap();
        std::fs::write(
            scripts.join("app.luau"),
            "local A = require(\"pkg_a/user\")\nlocal B = require(\"pkg_b/user\")\n\
             function go() ctx.a = A.who() ctx.b = B.who() end",
        )
        .unwrap();
        std::fs::write(
            pa.join("user.luau"),
            "local X = require(\"x\")\nreturn { who = function() return X.name end }",
        )
        .unwrap();
        std::fs::write(
            pb.join("user.luau"),
            "local X = require(\"x\")\nreturn { who = function() return X.name end }",
        )
        .unwrap();
        std::fs::write(pa.join("x.luau"), "return { name = \"de-a\" }").unwrap();
        std::fs::write(pb.join("x.luau"), "return { name = \"de-b\" }").unwrap();

        let comp = LuauComponent::from_file(dir.join("app.gv").to_str().unwrap(), "app").unwrap();
        let data = drive(&comp, "go", None, HashMap::new());
        assert_eq!(data.get("a").map(String::as_str), Some("de-a"));
        assert_eq!(data.get("b").map(String::as_str), Some("de-b"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exemplo_imports_luau_carrega_e_importa_os_modulos() {
        // Exercita a árvore REAL do exemplo: app.gv -> script.luau, que faz
        // require("net.http_client") e require("util.strings"). Se algum caminho
        // quebrar, `from_file` (que roda o script no load) falha aqui.
        let comp = LuauComponent::from_file("examples/imports_luau/app.gv", "app").unwrap();
        // init() não usa rede; só semeia o estado — prova que os módulos
        // resolveram e o script rodou.
        let data = drive(&comp, "init", None, HashMap::new());
        assert_eq!(data.get("status").map(String::as_str), Some("pronto"));
    }

    #[test]
    fn resolve_module_acha_arquivo_e_init() {
        let dir = temp_dir("resolve");
        std::fs::create_dir_all(dir.join("pkg")).unwrap();
        std::fs::write(dir.join("solo.luau"), "return 1").unwrap();
        std::fs::write(dir.join("pkg").join("init.luau"), "return 2").unwrap();
        let roots = vec![dir.clone()];
        assert_eq!(
            resolve_module("solo", &roots, &DiskAssets),
            Some(dir.join("solo.luau"))
        );
        assert_eq!(
            resolve_module("pkg", &roots, &DiskAssets),
            Some(dir.join("pkg").join("init.luau"))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extrai_script_de_xml() {
        assert_eq!(
            extract_script("<Text/>\n<script> a </script>").as_deref(),
            Some(" a ")
        );
        // Sem bloco <script>: nada a extrair.
        assert_eq!(extract_script("<Text/>"), None);
    }

    #[test]
    fn navigate_pede_navegacao_ao_motor() {
        let comp = LuauComponent::from_source("function ir() navigate('perfil') end", "t.gv", "c")
            .unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.run("ir", None, &mut ctx);
        match ctx.nav {
            Some(crate::component::Nav::To(ref s)) => assert_eq!(s, "perfil"),
            _ => panic!("esperava Nav::To(\"perfil\")"),
        }
    }

    #[test]
    fn navigate_back_pede_volta_ao_motor() {
        let comp = LuauComponent::from_source("function voltar() navigate_back() end", "t.gv", "c")
            .unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.run("voltar", None, &mut ctx);
        assert!(matches!(ctx.nav, Some(crate::component::Nav::Back)));
    }

    #[test]
    fn open_window_pede_janela_ao_motor() {
        // Forma tabela: fonte `file` + título + tamanho.
        let comp = LuauComponent::from_source(
            "function abrir() open_window({ file = 'telas/detalhe.gv', title = 'Detalhe', width = 400, height = 300 }) end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.run("abrir", None, &mut ctx);
        assert_eq!(
            ctx.windows.len(),
            1,
            "open_window deveria enfileirar uma janela"
        );
        let spec = &ctx.windows[0];
        match &spec.source {
            crate::component::WindowSource::File(p) => assert_eq!(p, "telas/detalhe.gv"),
            _ => panic!("esperava WindowSource::File"),
        }
        assert_eq!(spec.title.as_deref(), Some("Detalhe"));
        assert_eq!(spec.size, Some((400.0, 300.0)));
    }

    #[test]
    fn open_window_string_vira_arquivo() {
        // Forma string curta: `open_window("...")` = `{ file = "..." }`.
        let comp = LuauComponent::from_source(
            "function abrir() open_window('telas/x.gv') end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.run("abrir", None, &mut ctx);
        assert_eq!(ctx.windows.len(), 1);
        assert!(
            matches!(&ctx.windows[0].source, crate::component::WindowSource::File(p) if p == "telas/x.gv")
        );
    }

    #[test]
    fn notify_pede_notificacao_do_so() {
        // Forma tabela: título + corpo viram uma NotificationSpec enfileirada.
        let comp = LuauComponent::from_source(
            "function avisar() notify({ title = 'Deploy', body = 'api no ar' }) end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.run("avisar", None, &mut ctx);
        assert_eq!(
            ctx.notifications.len(),
            1,
            "notify deveria enfileirar uma notificação"
        );
        assert_eq!(ctx.notifications[0].title, "Deploy");
        assert_eq!(ctx.notifications[0].body, "api no ar");
        assert_eq!(ctx.notifications[0].app_name, None);
        assert_eq!(ctx.notifications[0].icon, None);
    }

    #[test]
    fn notify_aceita_app_name_e_icon() {
        // `app_name`/`icon` opcionais chegam à NotificationSpec.
        let comp = LuauComponent::from_source(
            "function avisar() notify({ body = 'ok', app_name = 'Rustploy', icon = 'rustploy-gui' }) end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.run("avisar", None, &mut ctx);
        assert_eq!(ctx.notifications.len(), 1);
        assert_eq!(ctx.notifications[0].app_name.as_deref(), Some("Rustploy"));
        assert_eq!(ctx.notifications[0].icon.as_deref(), Some("rustploy-gui"));
    }

    #[test]
    fn notify_string_vira_corpo_sem_titulo() {
        // Forma string curta: `notify("...")` = `{ body = "..." }`, sem título.
        let comp =
            LuauComponent::from_source("function avisar() notify('build pronto') end", "t.gv", "c")
                .unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.run("avisar", None, &mut ctx);
        assert_eq!(ctx.notifications.len(), 1);
        assert_eq!(ctx.notifications[0].title, "");
        assert_eq!(ctx.notifications[0].body, "build pronto");
    }

    #[test]
    fn open_window_component_vira_named() {
        let comp = LuauComponent::from_source(
            "function abrir() open_window({ component = 'perfil' }) end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.run("abrir", None, &mut ctx);
        assert_eq!(ctx.windows.len(), 1);
        assert!(
            matches!(&ctx.windows[0].source, crate::component::WindowSource::Named(n) if n == "perfil")
        );
    }

    #[test]
    fn open_window_com_data_semeia_contexto_da_nova_janela() {
        // `data` vira pares no WindowSpec (o daemon/runtime os semeia no motor
        // da nova janela). Uma tabela dentro de `data` é serializada em JSON.
        let comp = LuauComponent::from_source(
            "function abrir() open_window({ file = 'f.gv', data = { url = 'http://x', n = 3 } }) end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.run("abrir", None, &mut ctx);
        assert_eq!(ctx.windows.len(), 1);
        let seeded = &ctx.windows[0].data;
        assert!(seeded.iter().any(|(k, v)| k == "url" && v == "http://x"));
        assert!(seeded.iter().any(|(k, v)| k == "n" && v == "3"));
    }

    #[test]
    fn broadcast_enfileira_mensagem_com_payload_json() {
        // `broadcast(event, tabela)` serializa a tabela em JSON no payload.
        let comp = LuauComponent::from_source(
            "function enviar() broadcast('project_created', { id = '42', name = 'api' }) end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.run("enviar", None, &mut ctx);
        assert_eq!(ctx.broadcasts.len(), 1);
        assert_eq!(ctx.broadcasts[0].event, "project_created");
        let json: serde_json::Value = serde_json::from_str(&ctx.broadcasts[0].payload).unwrap();
        assert_eq!(json["id"], "42");
        assert_eq!(json["name"], "api");
    }

    #[test]
    fn close_window_pede_fechar_a_propria_janela() {
        let comp =
            LuauComponent::from_source("function sair() close_window() end", "t.gv", "c").unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        assert!(!ctx.close_self);
        comp.run("sair", None, &mut ctx);
        assert!(ctx.close_self);
    }

    #[test]
    fn on_broadcast_recebe_evento_e_payload_decodificado() {
        // O handler global `on_broadcast(event, payload)` recebe o payload já
        // decodificado de JSON numa tabela Lua.
        let comp = LuauComponent::from_source(
            "function on_broadcast(event, payload)\n\
             ctx.got_event = event\n\
             ctx.got_name = payload.name\n\
             end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.on_broadcast_inner("project_created", "{\"name\":\"api\"}", &mut ctx)
            .unwrap();
        assert_eq!(
            ctx.get("got_event").map(String::as_str),
            Some("project_created")
        );
        assert_eq!(ctx.get("got_name").map(String::as_str), Some("api"));
    }

    #[test]
    fn after_agenda_sem_suspender_e_dispara_o_handler() {
        let comp = LuauComponent::from_source(
            "function iniciar() after(50, 'disparou') end\n\
             function disparou() ctx.tocou = 'sim' end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();

        let id;
        {
            let mut ctx = Context::new(&mut data);
            comp.run("iniciar", None, &mut ctx);
            assert_eq!(
                ctx.timers.len(),
                1,
                "after não deveria suspender a corrotina"
            );
            assert_eq!(ctx.timers[0].delay_ms, 50);
            id = ctx.timers[0].id;
        }
        {
            let mut ctx = Context::new(&mut data);
            comp.resume_timer_inner(id, &mut ctx).unwrap();
        }
        assert_eq!(data.get("tocou").map(String::as_str), Some("sim"));
    }

    #[test]
    fn after_cancelado_pelo_handle_nao_dispara() {
        let comp = LuauComponent::from_source(
            "function iniciar() local t = after(50, 'disparou') t:cancel() end\n\
             function disparou() ctx.tocou = 'sim' end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();

        let id;
        {
            let mut ctx = Context::new(&mut data);
            comp.run("iniciar", None, &mut ctx);
            id = ctx.timers[0].id;
        }
        {
            let mut ctx = Context::new(&mut data);
            comp.resume_timer_inner(id, &mut ctx).unwrap();
        }
        assert_eq!(
            data.get("tocou"),
            None,
            "cancelado antes de vencer não deveria disparar"
        );
    }

    #[test]
    fn every_reagenda_apos_cada_disparo() {
        let comp = LuauComponent::from_source(
            "function iniciar() ctx.contador = 0 every(50, 'tique') end\n\
             function tique() ctx.contador = ctx.contador + 1 end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();

        let first_id;
        {
            let mut ctx = Context::new(&mut data);
            comp.run("iniciar", None, &mut ctx);
            assert_eq!(
                ctx.timers.len(),
                1,
                "every não deveria suspender a corrotina"
            );
            first_id = ctx.timers[0].id;
        }

        let second_id;
        {
            let mut ctx = Context::new(&mut data);
            comp.resume_timer_inner(first_id, &mut ctx).unwrap();
            assert_eq!(
                ctx.timers.len(),
                1,
                "cada disparo deveria reagendar o próximo"
            );
            second_id = ctx.timers[0].id;
        }
        assert_ne!(
            first_id, second_id,
            "a repetição usa um novo handle a cada disparo"
        );
        assert_eq!(data.get("contador").map(String::as_str), Some("1"));

        {
            let mut ctx = Context::new(&mut data);
            comp.resume_timer_inner(second_id, &mut ctx).unwrap();
        }
        assert_eq!(data.get("contador").map(String::as_str), Some("2"));
    }

    #[test]
    fn every_cancelado_para_de_reagendar() {
        let comp = LuauComponent::from_source(
            "function iniciar() ctx.contador = 0 handle = every(50, 'tique') end\n\
             function tique() ctx.contador = ctx.contador + 1 end\n\
             function parar() handle:cancel() end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();

        let first_id;
        {
            let mut ctx = Context::new(&mut data);
            comp.run("iniciar", None, &mut ctx);
            first_id = ctx.timers[0].id;
        }
        {
            let mut ctx = Context::new(&mut data);
            comp.resume_timer_inner(first_id, &mut ctx).unwrap();
        }
        assert_eq!(data.get("contador").map(String::as_str), Some("1"));

        // Cancela antes do próximo disparo: não deve reagendar nem incrementar de novo.
        {
            let mut ctx = Context::new(&mut data);
            comp.run("parar", None, &mut ctx);
        }
        assert_eq!(data.get("contador").map(String::as_str), Some("1"));
    }

    #[test]
    fn on_error_hook_e_chamado_quando_definido_em_vez_de_promover_a_toast() {
        let comp = LuauComponent::from_source(
            "function quebra() error('deu ruim') end\n\
             function on_error(msg) ctx.capturado = msg end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        {
            let mut ctx = Context::new(&mut data);
            comp.run("quebra", None, &mut ctx);
            assert!(
                ctx.toasts.is_empty(),
                "on_error definido não deveria também promover a um toast"
            );
        }
        assert!(
            data.get("capturado")
                .map(|s| s.contains("deu ruim"))
                .unwrap_or(false),
            "on_error deveria ter recebido a mensagem do erro"
        );
    }

    #[test]
    fn erro_sem_on_error_e_promovido_a_toast() {
        let comp =
            LuauComponent::from_source("function quebra() error('deu ruim') end", "t.gv", "c")
                .unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.run("quebra", None, &mut ctx);
        assert_eq!(
            ctx.toasts.len(),
            1,
            "sem on_error, o erro deveria virar toast visível"
        );
        assert_eq!(ctx.toasts[0].kind, crate::toasts::ToastKind::Error);
    }

    #[test]
    fn ctx_aceita_tabela_serializando_via_json() {
        let comp = LuauComponent::from_source(
            "function ir() ctx.obj = { a = 1, b = 'x' } end",
            "t.gv",
            "c",
        )
        .unwrap();
        let data = drive(&comp, "ir", None, HashMap::new());
        let raw = data
            .get("obj")
            .expect("ctx.obj deveria ter sido gravado (serializado como JSON)");
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert_eq!(v["a"], 1);
        assert_eq!(v["b"], "x");
    }

    #[test]
    fn ctx_com_funcao_dentro_da_tabela_e_descartado_sem_afetar_o_resto() {
        let comp = LuauComponent::from_source(
            "function ir() ctx.ruim = { f = function() end } ctx.bom = 'ok' end",
            "t.gv",
            "c",
        )
        .unwrap();
        let data = drive(&comp, "ir", None, HashMap::new());
        assert_eq!(
            data.get("ruim"),
            None,
            "tabela com função dentro não é serializável"
        );
        assert_eq!(data.get("bom").map(String::as_str), Some("ok"));
    }

    #[test]
    fn viewport_reflete_o_tamanho_atual_do_motor() {
        let comp = LuauComponent::from_source(
            "function ler() local v = viewport() ctx.w = v.width ctx.h = v.height end",
            "t.gv",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        {
            let mut ctx = Context::new(&mut data);
            ctx.set_viewport((1024.0, 768.0));
            comp.run("ler", None, &mut ctx);
        }
        assert_eq!(data.get("w").map(String::as_str), Some("1024"));
        assert_eq!(data.get("h").map(String::as_str), Some("768"));
    }

    #[test]
    fn storage_persiste_entre_instancias_do_componente() {
        let dir = temp_dir("storage");
        let path = dir.join("t.gv");
        let script = "function salvar() storage.set('contador', ctx.contador) end\n\
                      function carregar() ctx.contador = storage.get('contador') end";

        let comp1 = LuauComponent::from_source(script, path.to_str().unwrap(), "app").unwrap();
        let mut data = HashMap::new();
        data.insert("contador".into(), "7".into());
        let _ = drive(&comp1, "salvar", None, data);

        // Nova instância (simula reiniciar o processo): mesmo `path`/nome, então
        // o mesmo arquivo `.glacier-storage/app.json` — deveria ler o valor salvo.
        let comp2 = LuauComponent::from_source(script, path.to_str().unwrap(), "app").unwrap();
        let data2 = drive(&comp2, "carregar", None, HashMap::new());
        assert_eq!(data2.get("contador").map(String::as_str), Some("7"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn storage_remove_apaga_a_chave() {
        let dir = temp_dir("storage_remove");
        let path = dir.join("t.gv");
        let comp = LuauComponent::from_source(
            "function fluxo()\n\
               storage.set('k', 'v')\n\
               storage.remove('k')\n\
               ctx.depois = storage.get('k')\n\
               ctx.tipo = type(ctx.depois)\n\
             end",
            path.to_str().unwrap(),
            "app",
        )
        .unwrap();
        let data = drive(&comp, "fluxo", None, HashMap::new());
        assert_eq!(
            data.get("depois"),
            None,
            "storage.get de uma chave removida deveria ser nil"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn storage_path_usa_a_raiz_do_app_quando_definida() {
        let module_base = Path::new("/opt/app/scripts/app.luau");

        // Sem raiz (legado): relativo ao diretório do script.
        assert_eq!(
            storage_path(module_base, "app", None),
            PathBuf::from("/opt/app/scripts/.glacier-storage/app.json")
        );

        // Com raiz do app: grava sob ela, ignorando o local do script — é o que
        // torna o `storage` gravável quando os assets moram num caminho
        // read-only. O nome do componente é sanitizado (`:` vira `_`).
        let root = Path::new("/home/u/.local/share/rustploy");
        assert_eq!(
            storage_path(module_base, "app:main", Some(root)),
            PathBuf::from("/home/u/.local/share/rustploy/.glacier-storage/app_main.json")
        );
    }

    #[test]
    fn sandbox_nao_expoe_io_nem_os_execute() {
        // Fixa o contrato de segurança implícito do Luau (dialeto Roblox): sem
        // `io`, sem `os.execute` — um script não deveria conseguir abrir
        // arquivo arbitrário nem rodar processo.
        let comp = LuauComponent::from_source(
            "function checar()\n\
               ctx.tem_io = tostring(io ~= nil)\n\
               local ok = pcall(function() return os.execute('echo oi') end)\n\
               ctx.os_execute_ok = tostring(ok)\n\
             end",
            "t.gv",
            "c",
        )
        .unwrap();
        let data = drive(&comp, "checar", None, HashMap::new());
        assert_eq!(
            data.get("tem_io").map(String::as_str),
            Some("false"),
            "io não deveria estar disponível"
        );
        assert_eq!(
            data.get("os_execute_ok").map(String::as_str),
            Some("false"),
            "os.execute não deveria existir/funcionar"
        );
    }

    #[test]
    fn exemplo_navegacao_luau_login_correto_navega_para_o_dashboard() {
        let comp =
            LuauComponent::from_file("examples/navegacao_luau/login.gv", "login_luau").unwrap();
        let mut data = HashMap::new();
        data.insert("usuario".into(), "admin".into());
        data.insert("senha".into(), "123".into());
        let mut ctx = Context::new(&mut data);
        comp.run("entrar", None, &mut ctx);
        match ctx.nav {
            Some(crate::component::Nav::To(ref s)) => assert_eq!(s, "dashboard_luau"),
            _ => panic!("credenciais corretas deveriam navegar para dashboard_luau"),
        }
    }

    #[test]
    fn exemplo_navegacao_luau_login_errado_nao_navega_e_seta_erro() {
        let comp =
            LuauComponent::from_file("examples/navegacao_luau/login.gv", "login_luau").unwrap();
        let mut data = HashMap::new();
        data.insert("usuario".into(), "quemquer".into());
        data.insert("senha".into(), "errada".into());
        {
            let mut ctx = Context::new(&mut data);
            comp.run("entrar", None, &mut ctx);
            assert!(
                ctx.nav.is_none(),
                "credenciais erradas não deveriam navegar"
            );
        }
        assert!(data.get("erro").map(|s| !s.is_empty()).unwrap_or(false));
    }

    #[test]
    fn exemplo_navegacao_luau_dashboard_sai_volta_e_limpa_senha() {
        let comp =
            LuauComponent::from_file("examples/navegacao_luau/dashboard.gv", "dashboard_luau")
                .unwrap();
        let mut data = HashMap::new();
        data.insert("senha".into(), "123".into());
        {
            let mut ctx = Context::new(&mut data);
            comp.run("sair", None, &mut ctx);
            assert!(matches!(ctx.nav, Some(crate::component::Nav::Back)));
        }
        assert_eq!(data.get("senha"), None, "sair deveria limpar ctx.senha");
    }

    #[test]
    fn exemplo_robustez_luau_exercita_timers_storage_viewport_ctx_tabela_e_erro() {
        let storage_file = PathBuf::from("examples/robustez_luau/.glacier-storage/robustez.json");
        let _ = std::fs::remove_file(&storage_file);

        let comp =
            LuauComponent::from_file("examples/robustez_luau/robustez.gv", "robustez").unwrap();
        let mut data = HashMap::new();

        // init() lê o storage (vazio na primeira vez) e semeia os defaults.
        {
            let mut ctx = Context::new(&mut data);
            comp.run("init", None, &mut ctx);
        }
        assert_eq!(data.get("rascunho").map(String::as_str), Some(""));

        // after(): agenda sem suspender; cancelar impede o disparo.
        {
            let mut ctx = Context::new(&mut data);
            comp.run("iniciar_temporizador", None, &mut ctx);
            assert_eq!(ctx.timers.len(), 1);
        }
        {
            let mut ctx = Context::new(&mut data);
            comp.run("cancelar_temporizador", None, &mut ctx);
        }
        assert_eq!(data.get("status").map(String::as_str), Some("cancelado"));

        // viewport(): reflete o que o motor informou via Context::set_viewport.
        {
            let mut ctx = Context::new(&mut data);
            ctx.set_viewport((800.0, 600.0));
            comp.run("ler_viewport", None, &mut ctx);
        }
        assert_eq!(data.get("largura").map(String::as_str), Some("800"));
        assert_eq!(data.get("altura").map(String::as_str), Some("600"));

        // ctx aceita tabela: gerar_prefs grava JSON, não desaparece.
        {
            let mut ctx = Context::new(&mut data);
            comp.run("gerar_prefs", None, &mut ctx);
        }
        let prefs: serde_json::Value =
            serde_json::from_str(data.get("prefs").expect("prefs deveria ter sido gravado"))
                .unwrap();
        assert_eq!(prefs["tema"], "escuro");
        assert_eq!(prefs["volume"], 7);

        // erro visível: provocar_erro falha, on_error captura e mostra um toast.
        {
            let mut ctx = Context::new(&mut data);
            comp.run("provocar_erro", None, &mut ctx);
            assert_eq!(
                ctx.toasts.len(),
                1,
                "on_error do exemplo deveria mostrar um toast"
            );
        }
        assert!(
            data.get("ultimo_erro")
                .map(|s| s != "(nenhum ainda)")
                .unwrap_or(false)
        );

        // storage: salvar e reler numa instância NOVA (simula reiniciar o app).
        data.insert("rascunho".into(), "anotação importante".into());
        {
            let mut ctx = Context::new(&mut data);
            comp.run("salvar_rascunho", None, &mut ctx);
        }
        let comp2 =
            LuauComponent::from_file("examples/robustez_luau/robustez.gv", "robustez").unwrap();
        let mut data2 = HashMap::new();
        {
            let mut ctx2 = Context::new(&mut data2);
            comp2.run("init", None, &mut ctx2);
        }
        assert_eq!(
            data2.get("rascunho").map(String::as_str),
            Some("anotação importante")
        );

        let _ = std::fs::remove_file(&storage_file);
    }
}
