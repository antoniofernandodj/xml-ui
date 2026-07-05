//! Comportamento de componente escrito em **Lua**, interpretado em tempo de
//! execução — sem etapa de compilação.
//!
//! O bloco `<script>` de um template guarda **Lua** (5.4, via [`mlua`]):
//! [`LuaComponent`] o carrega do arquivo e executa as funções quando uma ação
//! chega — nada é compilado, então mudar a lógica não exige recompilar o app.
//!
//! # Acesso ao contexto
//!
//! Cada função Lua enxerga uma tabela global `ctx` espelhando o
//! [`Context`](crate::Context) do motor. Ler `ctx.contador` devolve o valor
//! atual (string); atribuir `ctx.contador = ...` grava de volta. Como Lua
//! coage strings numéricas em aritmética, um contador é só:
//!
//! ```lua
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
//! ```lua
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
//! ```lua
//! local http = require("net.http_client")   -- net/http_client.lua
//! local api  = http.new("https://api.exemplo")
//!
//! function carregar()
//!     local res = api:get("/dados")          -- o módulo pode usar fetch (async)
//!     if res.ok then ctx.dados = res.body end
//! end
//! ```
//!
//! `require("a.b")` procura `a/b.lua` (e `a/b/init.lua`) resolvido, na ordem:
//! diretório do template → `<dir>/lib` → cada caminho em `GLACIER_LUA_PATH`
//! (separados por `:`). Módulos rodam no **mesmo** interpretador do componente,
//! então enxergam `fetch` e são carregados uma única vez (cacheados como no Lua
//! padrão). Ver [`install_module_system`].

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::component::{
    Component, Context, FetchResult, PendingFetch, StreamCommand, StreamCommandKind,
    StreamEventKind, StreamKind, StreamRequest, Template,
};
use mlua::{Function, Lua, MultiValue, RegistryKey, Table, Thread, ThreadStatus, Value};

/// Prelúdio Lua injetado antes do `<script>` do usuário. Define `fetch` e,
/// para streams de vida longa, `sse` / `websocket` (ver [`LuaComponent::drive`]).
///
/// `fetch` **suspende** a corrotina até a resposta (aparência de `await`).
/// `sse`/`websocket` **não** suspendem: cedem um pedido de abertura, o motor
/// registra o stream e retoma na hora devolvendo um handle. A partir daí os
/// eventos chegam pelos handlers nomeados em `opts` (`on_message`, `on_open`,
/// `on_error`, `on_close`), e o handle permite enviar/fechar:
///
/// ```lua
/// function init()
///     conn = websocket("wss://ex/ws", { on_message = "on_msg" })
/// end
/// function on_msg(data) ctx.ultima = data end
/// function enviar() conn:send(ctx.texto) end
/// ```
const PRELUDE: &str = r#"
function fetch(url, opts)
    return coroutine.yield({ __glacier_fetch = true, url = url, opts = opts or {} })
end

function sse(url, opts)
    local id = coroutine.yield({ __glacier_stream_open = true, kind = "sse", url = url, opts = opts or {} })
    return {
        id = id,
        close = function(self)
            return coroutine.yield({ __glacier_stream_cmd = true, id = self.id, cmd = "close" })
        end,
    }
end

function websocket(url, opts)
    local id = coroutine.yield({ __glacier_stream_open = true, kind = "ws", url = url, opts = opts or {} })
    return {
        id = id,
        send = function(self, text)
            return coroutine.yield({ __glacier_stream_cmd = true, id = self.id, cmd = "send", text = text })
        end,
        close = function(self)
            return coroutine.yield({ __glacier_stream_cmd = true, id = self.id, cmd = "close" })
        end,
    }
end
"#;

/// Um [`Component`] cujo comportamento vem de um bloco `<script>` em Lua.
///
/// O template (XML ou KDL) é lido do disco; seu `<script>` é extraído e
/// carregado num interpretador Lua próprio. Cada ação (`onClick`, `onChange`,
/// `onSubmit`) roda como uma **corrotina**: chama a função Lua homônima, que
/// lê/escreve o contexto via a tabela global `ctx` e pode chamar `fetch` para
/// rede sem bloquear a UI.
pub struct LuaComponent {
    name: String,
    path: String,
    lua: Lua,
    /// Tabela `ctx` persistente (o mesmo objeto entre chamadas), espelhando o
    /// contexto do motor. Mantida fixa para que corrotinas suspensas que a
    /// referenciam continuem válidas ao serem retomadas.
    ctx_table: Table,
    /// Corrotinas suspensas num `fetch`, aguardando a resposta, por `id`.
    pending: RefCell<HashMap<u64, Thread>>,
    /// Streams de vida longa abertos (`sse`/`websocket`), por `id`: os handlers
    /// Lua registrados (`on_message`, …) que o motor chama a cada evento.
    streams: RefCell<HashMap<u64, StreamRegistration>>,
    /// Gerador de `id`, compartilhado por `fetch`es e streams (ids únicos no
    /// componente).
    next_id: Cell<u64>,
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

impl LuaComponent {
    /// Cria um componente Lua a partir de um arquivo de template.
    ///
    /// O corpo Lua vem de uma de duas fontes:
    /// - **externo**: `<script src="arquivo.lua">` (ou `from="..."`) carrega o
    ///   Lua de outro arquivo, resolvido relativo ao diretório do template;
    /// - **inline**: senão, o corpo do próprio bloco `<script>...</script>`.
    ///
    /// O script é executado uma vez para definir as funções. Erros de I/O ou de
    /// sintaxe Lua viram `Err`.
    pub fn from_file(path: impl Into<String>, name: impl Into<String>) -> Result<Self, String> {
        let path = path.into();
        let name = name.into();
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Falha ao ler template Lua em '{}': {}", path, e))?;
        let script = resolve_script(&content, &path)?;
        Self::from_source(&script, path, name)
    }

    /// Cria um componente Lua a partir do código-fonte já extraído, associando-o
    /// a um `path` de template (para o motor renderizar a UI e manter hot-reload).
    pub fn from_source(
        script: &str,
        path: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Self, String> {
        let name = name.into();
        let path = path.into();
        let lua = Lua::new();
        lua.load(PRELUDE).set_name("<glacier prelude>").exec().map_err(|e| {
            format!("Erro ao carregar prelúdio Lua: {}", e)
        })?;
        // Habilita `require(...)` resolvendo módulos relativo ao template, para
        // que o `<script>` possa importar bibliotecas (ex.: clients de rede).
        install_module_system(&lua, module_roots(&path))
            .map_err(|e| format!("Erro ao instalar sistema de módulos Lua: {}", e))?;
        lua.load(script)
            .set_name(format!("<script:{name}>"))
            .exec()
            .map_err(|e| format!("Erro ao carregar <script> Lua de '{}': {}", name, e))?;
        let ctx_table = lua
            .create_table()
            .map_err(|e| format!("Erro ao criar tabela ctx: {}", e))?;
        lua.globals()
            .set("ctx", &ctx_table)
            .map_err(|e| format!("Erro ao registrar ctx: {}", e))?;
        Ok(Self {
            name,
            path,
            lua,
            ctx_table,
            pending: RefCell::new(HashMap::new()),
            streams: RefCell::new(HashMap::new()),
            next_id: Cell::new(1),
        })
    }

    /// Espelha o contexto do motor na tabela Lua `ctx`: limpa a tabela e a
    /// repopula com o estado atual, para que ela reflita o contexto *exatamente*
    /// no início da execução. É o que permite ao `sync_from_lua` detectar o que
    /// o Lua removeu (`ctx.x = nil`). A tabela é limpa in-place (mesmo objeto),
    /// preservando referências de corrotinas suspensas.
    fn sync_to_lua(&self, ctx: &Context) -> mlua::Result<()> {
        self.ctx_table.clear()?;
        for (k, v) in ctx.data.iter() {
            self.ctx_table.set(k.as_str(), v.as_str())?;
        }
        Ok(())
    }

    /// Copia a tabela `ctx` de volta ao contexto do motor, tratando-a como a
    /// fonte da verdade: chaves com valor string-izável são gravadas (novas
    /// incluídas); chaves que o Lua apagou (`ctx.x = nil`) são **removidas** do
    /// contexto — como a tabela começou espelhando o contexto (ver
    /// [`Self::sync_to_lua`]), toda chave do contexto ausente aqui foi
    /// deliberadamente removida pelo script. `nil`/tabelas/funções não são
    /// gravados.
    fn sync_from_lua(&self, ctx: &mut Context) -> mlua::Result<()> {
        let mut present = std::collections::HashSet::new();
        for pair in self.ctx_table.pairs::<String, Value>() {
            let (k, val) = pair?;
            present.insert(k.clone());
            if let Some(s) = lua_value_to_string(&val) {
                ctx.set(&k, s);
            }
        }
        // Chaves que existiam no contexto mas não estão mais na tabela (o Lua as
        // setou para nil) são removidas.
        let removed: Vec<String> =
            ctx.data.keys().filter(|k| !present.contains(*k)).cloned().collect();
        for k in removed {
            ctx.data.remove(&k);
        }
        Ok(())
    }

    /// Roda a função `func` (se existir) como uma corrotina, passando `value`.
    fn run(&self, func: &str, value: Option<&str>, ctx: &mut Context) {
        if let Err(e) = self.run_inner(func, value, ctx) {
            eprintln!("[glacier-ui] erro em <script> Lua '{}::{}': {}", self.name, func, e);
        }
    }

    fn run_inner(&self, func: &str, value: Option<&str>, ctx: &mut Context) -> mlua::Result<()> {
        self.sync_to_lua(ctx)?;
        self.lua.globals().set("value", value)?;

        // Ações sem função correspondente são ignoradas (como o `_ => {}` antigo).
        let Ok(f) = self.lua.globals().get::<Function>(func) else {
            return Ok(());
        };
        let thread = self.lua.create_thread(f)?;
        let args = match value {
            Some(v) => MultiValue::from_iter([Value::String(self.lua.create_string(v)?)]),
            None => MultiValue::new(),
        };
        self.drive(thread, args, ctx)
    }

    /// Retoma a corrotina suspensa `id` com o resultado do `fetch`.
    fn resume_inner(&self, id: u64, result: &FetchResult, ctx: &mut Context) -> mlua::Result<()> {
        let Some(thread) = self.pending.borrow_mut().remove(&id) else {
            return Ok(());
        };
        self.sync_to_lua(ctx)?;
        let res = self.result_to_lua(result)?;
        self.drive(thread, MultiValue::from_iter([Value::Table(res)]), ctx)
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
    ///   **retoma na hora** devolvendo o `id` (o Lua recebe o handle).
    /// - `__glacier_stream_cmd`: registra o comando de saída (`send`/`close`) e
    ///   **retoma na hora**.
    ///
    /// Só `fetch` suspende de verdade; stream-open/cmd continuam o mesmo turno.
    fn drive(&self, thread: Thread, mut args: MultiValue, ctx: &mut Context) -> mlua::Result<()> {
        loop {
            let yielded: MultiValue = thread.resume(args)?;
            self.sync_from_lua(ctx)?;

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
                StreamRegistration { on_open: None, on_message: None, on_error: None, on_close: None },
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
            Value::Function(f) => Ok(Some(self.lua.create_registry_value(f)?)),
            Value::String(s) => match self.lua.globals().get::<Value>(s.to_string_lossy())? {
                Value::Function(f) => Ok(Some(self.lua.create_registry_value(f)?)),
                _ => Ok(None),
            },
            _ => Ok(None),
        }
    }

    /// Extrai o [`StreamCommand`] (`send`/`close`) que o handle Lua cedeu.
    fn record_stream_cmd(&self, req: &Table, ctx: &mut Context) -> mlua::Result<()> {
        let id: u64 = req.get("id")?;
        let kind = match req.get::<String>("cmd")?.as_str() {
            "close" => StreamCommandKind::Close,
            _ => StreamCommandKind::Send,
        };
        let text: Option<String> = req.get("text")?;
        ctx.stream_cmds.push(StreamCommand::new(id, kind, text.unwrap_or_default()));
        Ok(())
    }

    /// Entrega um evento de stream ao handler Lua registrado (se houver),
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
                Some(key) => Some(self.lua.registry_value(key)?),
                None => None,
            }
        };
        if kind == StreamEventKind::Closed {
            self.streams.borrow_mut().remove(&id);
        }
        let Some(func) = func else { return Ok(()) };

        self.sync_to_lua(ctx)?;
        self.lua.globals().set("value", data)?;
        let thread = self.lua.create_thread(func)?;
        let args = MultiValue::from_iter([Value::String(self.lua.create_string(data)?)]);
        self.drive(thread, args, ctx)
    }

    /// Extrai uma [`PendingFetch`] da tabela `{ url, opts }` que o `fetch` cedeu.
    fn parse_fetch(&self, id: u64, req: &Table) -> mlua::Result<PendingFetch> {
        let url: String = req.get("url")?;
        let opts: Option<Table> = req.get("opts")?;
        let (method, body, headers) = match opts {
            Some(o) => {
                let method = o.get::<Option<String>>("method")?.unwrap_or_else(|| "GET".into());
                let body = o.get::<Option<String>>("body")?;
                let headers = parse_headers_table(&o)?;
                (method, body, headers)
            }
            None => ("GET".into(), None, Vec::new()),
        };
        Ok(PendingFetch::new(id, url, method, body, headers))
    }

    /// Converte um [`FetchResult`] na tabela Lua `{ ok, status, body, error }`.
    fn result_to_lua(&self, r: &FetchResult) -> mlua::Result<Table> {
        let t = self.lua.create_table()?;
        t.set("ok", r.ok)?;
        t.set("status", r.status)?;
        t.set("body", r.body.as_str())?;
        t.set("error", r.error.as_str())?;
        Ok(t)
    }
}

impl Component for LuaComponent {
    fn name(&self) -> &str {
        &self.name
    }

    fn template(&self) -> Template {
        Template::File(self.path.clone())
    }

    /// Chama uma função Lua opcional `init()` para semear o estado inicial.
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
            eprintln!("[glacier-ui] erro ao retomar fetch em '{}': {}", self.name, e);
        }
    }

    fn on_stream_event(&mut self, id: u64, kind: StreamEventKind, data: &str, ctx: &mut Context) {
        if let Err(e) = self.on_stream_event_inner(id, kind, data, ctx) {
            eprintln!("[glacier-ui] erro em stream Lua '{}' (id {}): {}", self.name, id, e);
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

/// Converte um [`Value`] Lua na string que o contexto do motor guarda. Números
/// inteiros e floats de valor inteiro viram `"3"` (não `"3.0"`); `nil` devolve
/// `None` para não sobrescrever chaves com valor vazio à toa.
fn lua_value_to_string(v: &Value) -> Option<String> {
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
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Roda `func`/`value` contra um mapa de contexto e devolve o mapa mutado,
    /// exercitando o mesmo caminho de `update`.
    fn drive(comp: &LuaComponent, func: &str, value: Option<&str>, mut data: HashMap<String, String>) -> HashMap<String, String> {
        let mut ctx = Context::new(&mut data);
        comp.run(func, value, &mut ctx);
        data
    }

    #[test]
    fn incrementa_lendo_e_escrevendo_o_contexto() {
        let comp = LuaComponent::from_source(
            "function incrementar() ctx.contador = ctx.contador + 1 end",
            "t.xml",
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
        let comp = LuaComponent::from_source(
            "function set_nome(v) ctx.nome = v end",
            "t.xml",
            "c",
        )
        .unwrap();
        let data = drive(&comp, "set_nome", Some("Ana"), HashMap::new());
        assert_eq!(data.get("nome").map(String::as_str), Some("Ana"));
    }

    #[test]
    fn atribuir_nil_remove_a_chave_no_contexto() {
        let comp = LuaComponent::from_source(
            "function limpar() ctx.temp = nil end",
            "t.xml",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        data.insert("temp".into(), "algo".into());
        data.insert("manter".into(), "ok".into());
        let data = drive(&comp, "limpar", None, data);
        assert_eq!(data.get("temp"), None, "ctx.temp = nil deveria remover a chave");
        // Chaves não tocadas pelo script permanecem.
        assert_eq!(data.get("manter").map(String::as_str), Some("ok"));
    }

    #[test]
    fn acao_sem_funcao_e_ignorada() {
        let comp = LuaComponent::from_source("function a() end", "t.xml", "c").unwrap();
        let mut data = HashMap::new();
        data.insert("x".into(), "keep".into());
        let data = drive(&comp, "inexistente", None, data);
        assert_eq!(data.get("x").map(String::as_str), Some("keep"));
    }

    #[test]
    fn init_semea_default() {
        let comp = LuaComponent::from_source(
            "function init() ctx.contador = ctx.contador or 5 end",
            "t.xml",
            "c",
        )
        .unwrap();
        let data = drive(&comp, "init", None, HashMap::new());
        assert_eq!(data.get("contador").map(String::as_str), Some("5"));
    }

    #[test]
    fn fetch_suspende_a_corrotina_e_retoma_com_a_resposta() {
        let comp = LuaComponent::from_source(
            r#"
            function carregar()
                local res = fetch("http://exemplo/api", { method = "POST", body = "q" })
                if res.ok then ctx.dados = res.body else ctx.erro = res.error end
            end
            "#,
            "t.xml",
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
            let res = FetchResult { ok: true, status: 200, body: "OLA".into(), error: String::new() };
            comp.resume_inner(id, &res, &mut ctx).unwrap();
        }
        assert_eq!(data.get("dados").map(String::as_str), Some("OLA"));
        assert_eq!(data.get("erro"), None);
    }

    #[test]
    fn sse_registra_stream_sem_suspender_e_handler_recebe_mensagens() {
        let mut comp = LuaComponent::from_source(
            "function init() sse('http://ex/stream', { on_message = 'on_ev', on_close = 'on_fim' }) end\n\
             function on_ev(d) ctx.ultima = d end\n\
             function on_fim() ctx.fim = 'sim' end",
            "t.xml",
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
        assert!(comp.streams.borrow().is_empty(), "Closed deveria limpar o registro");
    }

    #[test]
    fn websocket_handle_envia_comando_de_saida() {
        let comp = LuaComponent::from_source(
            "function go()\n\
               local c = websocket('ws://ex', { on_message = 'on_ev' })\n\
               c:send('ola')\n\
             end\n\
             function on_ev(d) ctx.ultima = d end",
            "t.xml",
            "c",
        )
        .unwrap();
        let mut data = HashMap::new();
        let mut ctx = Context::new(&mut data);
        comp.run("go", None, &mut ctx);
        // Abriu um WebSocket e enfileirou um `send` — sem suspender.
        assert_eq!(ctx.streams.len(), 1);
        assert_eq!(ctx.streams[0].kind, StreamKind::Ws);
        assert_eq!(ctx.stream_cmds.len(), 1, "c:send deveria enfileirar 1 comando");
        assert_eq!(ctx.stream_cmds[0].kind, StreamCommandKind::Send);
        assert_eq!(ctx.stream_cmds[0].text, "ola");
        // O comando referencia o mesmo id do stream aberto.
        assert_eq!(ctx.stream_cmds[0].id, ctx.streams[0].id);
    }

    #[test]
    fn detecta_src_externo() {
        assert_eq!(
            extract_script_src(r#"<script src="scripts/c.lua"></script>"#).as_deref(),
            Some("scripts/c.lua")
        );
        assert_eq!(
            extract_script_src(r#"<script from='a.lua' />"#).as_deref(),
            Some("a.lua")
        );
        // Sem src: inline, então None.
        assert_eq!(extract_script_src("<script> a </script>"), None);
    }

    #[test]
    fn carrega_lua_de_arquivo_externo_relativo_ao_template() {
        // Monta template + .lua num diretório temporário e confere que o `src`
        // é resolvido relativo ao diretório do template.
        let dir = std::env::temp_dir().join(format!("glacier_lua_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tpl = dir.join("t.xml");
        let lua = dir.join("beh.lua");
        std::fs::write(&lua, "function incrementar() ctx.n = ctx.n + 1 end").unwrap();
        std::fs::write(&tpl, r#"<Text/><script src="beh.lua"></script>"#).unwrap();

        let comp = LuaComponent::from_file(tpl.to_str().unwrap(), "c").unwrap();
        let mut data = HashMap::new();
        data.insert("n".into(), "41".into());
        let data = drive(&comp, "incrementar", None, data);
        assert_eq!(data.get("n").map(String::as_str), Some("42"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Cria um diretório temporário exclusivo do teste (isolado por nome), útil
    /// para montar árvores de módulos.
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("glacier_lua_{}_{}", std::process::id(), tag));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn require_carrega_modulo_relativo_ao_template() {
        let dir = temp_dir("require_rel");
        std::fs::create_dir_all(dir.join("util")).unwrap();
        // Biblioteca pura: sem rede, só lógica encapsulada.
        std::fs::write(
            dir.join("util").join("strings.lua"),
            "local M = {}\nfunction M.shout(s) return s:upper() .. '!' end\nreturn M\n",
        )
        .unwrap();
        let comp = LuaComponent::from_source(
            "local strings = require('util.strings')\n\
             function grita() ctx.msg = strings.shout(ctx.msg) end",
            dir.join("t.xml").to_str().unwrap(),
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
            dir.join("once.lua"),
            "_G.__cargas = (_G.__cargas or 0) + 1\nreturn { n = _G.__cargas }\n",
        )
        .unwrap();
        let comp = LuaComponent::from_source(
            "local a = require('once')\nlocal b = require('once')\n\
             function checar() ctx.cargas = a.n ctx.mesmo = tostring(a == b) end",
            dir.join("t.xml").to_str().unwrap(),
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
            dir.join("net").join("client.lua"),
            "local Client = {}\nClient.__index = Client\n\
             function Client.new(base) return setmetatable({ base = base }, Client) end\n\
             function Client:get(p) return fetch(self.base .. p) end\n\
             return Client\n",
        )
        .unwrap();
        let comp = LuaComponent::from_source(
            "local http = require('net.client')\nlocal api = http.new('http://ex')\n\
             function carregar() local r = api:get('/x') if r.ok then ctx.dados = r.body end end",
            dir.join("t.xml").to_str().unwrap(),
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
            assert_eq!(ctx.fetches.len(), 1, "o módulo deveria ter suspendido num fetch");
            assert_eq!(ctx.fetches[0].url, "http://ex/x");
            id = ctx.fetches[0].id;
        }
        {
            let mut ctx = Context::new(&mut data);
            let res = FetchResult { ok: true, status: 200, body: "PONG".into(), error: String::new() };
            comp.resume_inner(id, &res, &mut ctx).unwrap();
        }
        assert_eq!(data.get("dados").map(String::as_str), Some("PONG"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn require_de_modulo_inexistente_falha_com_mensagem_clara() {
        let dir = temp_dir("require_missing");
        let comp = LuaComponent::from_source(
            "function usar() require('nao.existe') end",
            dir.join("t.xml").to_str().unwrap(),
            "c",
        )
        .unwrap();
        // Não deve derrubar o processo: o erro é logado e a ação vira no-op.
        let data = drive(&comp, "usar", None, HashMap::new());
        assert!(data.is_empty());
        // A resolução em si devolve None para um módulo ausente.
        assert!(resolve_module("nao.existe", &module_roots(dir.join("t.xml").to_str().unwrap())).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exemplo_imports_lua_carrega_e_importa_os_modulos() {
        // Exercita a árvore REAL do exemplo: app.xml -> script.lua, que faz
        // require("net.http_client") e require("util.strings"). Se algum caminho
        // quebrar, `from_file` (que roda o script no load) falha aqui.
        let comp = LuaComponent::from_file("examples/imports_lua/app.xml", "app").unwrap();
        // init() não usa rede; só semeia o estado — prova que os módulos
        // resolveram e o script rodou.
        let data = drive(&comp, "init", None, HashMap::new());
        assert_eq!(data.get("status").map(String::as_str), Some("pronto"));
    }

    #[test]
    fn resolve_module_acha_arquivo_e_init() {
        let dir = temp_dir("resolve");
        std::fs::create_dir_all(dir.join("pkg")).unwrap();
        std::fs::write(dir.join("solo.lua"), "return 1").unwrap();
        std::fs::write(dir.join("pkg").join("init.lua"), "return 2").unwrap();
        let roots = vec![dir.clone()];
        assert_eq!(resolve_module("solo", &roots), Some(dir.join("solo.lua")));
        assert_eq!(resolve_module("pkg", &roots), Some(dir.join("pkg").join("init.lua")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extrai_script_de_xml_e_kdl() {
        assert_eq!(
            extract_script("<Text/>\n<script> a </script>").as_deref(),
            Some(" a ")
        );
        // KDL: fecha no `}` de nível 0, respeitando chaves aninhadas do Lua.
        assert_eq!(
            extract_script("Text\nscript {\n if x then y() end\n}").as_deref(),
            Some("\n if x then y() end\n")
        );
    }
}

/// Chave (no registry do interpretador) da tabela que cacheia os módulos já
/// carregados por `require` — o equivalente ao `package.loaded` do Lua padrão,
/// mas privado ao motor.
const LOADED_KEY: &str = "glacier.loaded";

/// Diretórios onde `require` procura módulos, em ordem de prioridade, para um
/// template em `template_path`:
///
/// 1. o **diretório do template** (mesma convenção do `<script src="...">`);
/// 2. um subdiretório `lib/` desse diretório (convenção para código compartilhado);
/// 3. cada caminho em `GLACIER_LUA_PATH` (separados por `:`), para bibliotecas
///    fora da árvore do template.
fn module_roots(template_path: &str) -> Vec<PathBuf> {
    let base = Path::new(template_path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut roots = vec![base.clone(), base.join("lib")];
    if let Ok(extra) = std::env::var("GLACIER_LUA_PATH") {
        roots.extend(extra.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    }
    roots
}

/// Resolve o nome de módulo `a.b.c` para um arquivo `.lua`, testando
/// `a/b/c.lua` e depois `a/b/c/init.lua` em cada raiz, na ordem.
fn resolve_module(modname: &str, roots: &[PathBuf]) -> Option<PathBuf> {
    let rel = modname.replace('.', "/");
    for root in roots {
        let file = root.join(format!("{rel}.lua"));
        if file.is_file() {
            return Some(file);
        }
        let init = root.join(&rel).join("init.lua");
        if init.is_file() {
            return Some(init);
        }
    }
    None
}

/// Instala um `require` próprio no interpretador, resolvendo módulos pelas
/// `roots` (ver [`module_roots`]). Substitui o `require` padrão do Lua para dar
/// resolução previsível (relativa ao template, não ao diretório de trabalho),
/// erros claros e cache por interpretador.
///
/// Como o módulo roda no **mesmo** interpretador do componente, ele enxerga o
/// prelúdio (`fetch`) e as globais — um client de rede importado pode chamar
/// `fetch` e suspender a corrotina da ação como qualquer código inline. Cada
/// módulo é carregado uma vez; chamadas seguintes a `require` devolvem o valor
/// cacheado. O valor do módulo é o que seu arquivo `return`a (uma tabela, por
/// convenção); um módulo sem `return` é cacheado como `true`.
fn install_module_system(lua: &Lua, roots: Vec<PathBuf>) -> mlua::Result<()> {
    let cache = lua.create_table()?;
    lua.set_named_registry_value(LOADED_KEY, cache)?;

    let require = lua.create_function(move |lua, modname: String| {
        let cache: Table = lua.named_registry_value(LOADED_KEY)?;
        // Já carregado? Devolve o mesmo valor (identidade preservada).
        match cache.get::<Value>(modname.as_str())? {
            Value::Nil => {}
            cached => return Ok(cached),
        }

        let path = resolve_module(&modname, &roots).ok_or_else(|| {
            let procurados = roots
                .iter()
                .map(|r| r.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            mlua::Error::runtime(format!(
                "módulo Lua '{modname}' não encontrado (procurado como \
                 '{rel}.lua' e '{rel}/init.lua' em: {procurados})",
                rel = modname.replace('.', "/"),
            ))
        })?;

        let src = std::fs::read_to_string(&path).map_err(|e| {
            mlua::Error::runtime(format!(
                "falha ao ler módulo Lua '{modname}' ({}): {e}",
                path.display()
            ))
        })?;

        let value: Value = lua
            .load(&src)
            .set_name(format!("@{}", path.display()))
            .eval()?;
        // Módulo sem `return` explícito vira `true`, como no Lua padrão, para
        // não recarregar a cada chamada.
        let value = match value {
            Value::Nil => Value::Boolean(true),
            v => v,
        };
        cache.set(modname.as_str(), value.clone())?;
        Ok(value)
    })?;

    lua.globals().set("require", require)?;
    Ok(())
}

/// Resolve o corpo Lua de um template: se o `<script>` referencia um arquivo
/// externo via `src="..."` (ou `from="..."`), lê esse arquivo (caminho relativo
/// ao diretório do `template_path`); senão, usa o corpo inline do bloco.
fn resolve_script(markup: &str, template_path: &str) -> Result<String, String> {
    if let Some(src) = extract_script_src(markup) {
        let base = std::path::Path::new(template_path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let lua_path = base.join(&src);
        return std::fs::read_to_string(&lua_path).map_err(|e| {
            format!("Falha ao ler script Lua externo '{}': {}", lua_path.display(), e)
        });
    }
    Ok(extract_script(markup).unwrap_or_default())
}

/// Lê o atributo `src`/`from` da tag de abertura `<script ...>`, se houver — o
/// caminho de um arquivo `.lua` externo.
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

/// Extrai o corpo de um bloco `<script>...</script>` (XML) ou `script { ... }`
/// (KDL) de um template. Espelha a lógica de remoção do parser, mas devolve o
/// conteúdo em vez de descartá-lo.
fn extract_script(markup: &str) -> Option<String> {
    let lower = markup.to_ascii_lowercase();
    // XML: <script ...> corpo </script>
    if let Some(open) = lower.find("<script") {
        let gt = lower[open..].find('>')? + open + 1;
        let close = lower[gt..].find("</script>")? + gt;
        return Some(markup[gt..close].to_string());
    }
    // KDL: script { corpo }
    if let Some(rel) = lower.find("script") {
        let after = rel + "script".len();
        if let Some(brace_rel) = lower[after..].find('{') {
            let body_start = after + brace_rel + 1;
            // Fecha no `}` de nível 0 (o corpo Lua pode ter chaves aninhadas).
            let mut depth = 1i32;
            for (i, c) in markup[body_start..].char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(markup[body_start..body_start + i].to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    None
}
