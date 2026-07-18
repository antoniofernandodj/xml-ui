//! Camada HTTP mínima sobre [`hyper`], usada pelo `fetch` da camada Lua
//! (ver [`crate::luau`]). Faz uma requisição assíncrona (GET/POST/…) e devolve
//! um [`FetchResult`] — sem bloquear a thread de UI: o future roda no executor
//! do `iced` e seu resultado volta como [`crate::EngineMessage::LuauResume`].

use std::sync::OnceLock;

use futures::{SinkExt, Stream, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::Method;
use hyper::body::Bytes;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

use crate::component::{FetchResult, PendingFetch, StreamKind};

type HttpsClient = Client<HttpsConnector<HttpConnector>, Full<Bytes>>;

/// User-Agent padrão dos `fetch`, usado quando o chamador não define um (nem por
/// header nem pelo opt `user_agent`). Ver [`send`].
const DEFAULT_USER_AGENT: &str = concat!("glacier-ui/", env!("CARGO_PKG_VERSION"));

/// Instala o provider de criptografia default (ring) do rustls no processo, uma
/// única vez. Necessário porque o rustls 0.23 não embute um provider default:
/// tanto o cliente hyper quanto o handshake TLS do WebSocket
/// ([`tokio_tungstenite`]) leem esse default global. Idempotente — chamadas
/// seguintes são no-op.
pub(crate) fn install_default_crypto() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Cliente compartilhado (pool de conexões + config TLS), construído uma vez.
fn client() -> &'static HttpsClient {
    static CLIENT: OnceLock<HttpsClient> = OnceLock::new();
    CLIENT.get_or_init(|| {
        install_default_crypto();
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .build();
        Client::builder(TokioExecutor::new()).build(https)
    })
}

/// Executa a requisição descrita por `req` e devolve o resultado. Nunca entra
/// em pânico: qualquer erro (URL inválida, DNS, TLS, timeout de conexão…) vira
/// um [`FetchResult`] com `ok = false` e a mensagem em `error`.
pub(crate) async fn perform(req: PendingFetch) -> FetchResult {
    // `fetch("file://…")`: leitura de arquivo local em vez de requisição HTTP. O
    // Luau recebe o MESMO formato `{ ok, status, body, error }` de sempre, então
    // um `<script>` lê um arquivo com a mesma chamada com que faria um GET. Ao
    // contrário do browser (que bloqueia `file://` de propósito, por ser código
    // remoto não confiável), aqui o Luau é código do próprio app — acesso ao FS
    // local é esperado.
    if let Some(path) = req.url.strip_prefix("file://") {
        return read_file(path).await;
    }
    match send(&req).await {
        Ok(result) => result,
        Err(e) => FetchResult::error(e.to_string()),
    }
}

/// Lê um arquivo local para um [`FetchResult`], imitando a forma de uma resposta
/// HTTP: `200`/`ok` com o conteúdo no `body`, `404`/`error` quando o arquivo não
/// existe ou não pode ser lido. `tokio::fs` roda no executor async, sem bloquear
/// a thread de UI (igual ao caminho hyper).
async fn read_file(path: &str) -> FetchResult {
    match tokio::fs::read_to_string(path).await {
        Ok(body) => FetchResult {
            ok: true,
            status: 200,
            body,
            error: String::new(),
        },
        Err(e) => FetchResult {
            ok: false,
            status: 404,
            body: String::new(),
            error: e.to_string(),
        },
    }
}

async fn send(req: &PendingFetch) -> Result<FetchResult, Box<dyn std::error::Error + Send + Sync>> {
    let method = Method::from_bytes(req.method.to_uppercase().as_bytes())?;
    let body = Full::new(Bytes::from(req.body.clone().unwrap_or_default()));

    let mut builder = hyper::Request::builder().method(method).uri(&req.url);
    for (k, v) in &req.headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    // User-Agent padrão quando o chamador não definiu um (via header ou o opt
    // `user_agent` de `fetch`): alguns servidores rejeitam/roteiam diferente sem
    // UA. Um UA explícito nos headers sempre vence (não sobrescrevemos).
    if !req
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("user-agent"))
    {
        builder = builder.header(hyper::header::USER_AGENT, DEFAULT_USER_AGENT);
    }
    let request = builder.body(body)?;

    let response = client().request(request).await?;
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await?.to_bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();

    Ok(FetchResult {
        ok: (200..300).contains(&status),
        status,
        body: text,
        error: String::new(),
    })
}

// ---------------------------------------------------------------------------
// Streams de vida longa: SSE (Server-Sent Events) e WebSocket
// ---------------------------------------------------------------------------

/// Canal de saída para um WebSocket vivo: o motor guarda o `Sender` (emitido no
/// [`StreamEvent::Ready`]) e envia [`WsCommand`]s quando a camada Lua chama
/// `conn:send` / `conn:close`.
pub type WsSender = futures::channel::mpsc::Sender<WsCommand>;

/// Comando enviado à tarefa que mantém o WebSocket vivo.
#[derive(Debug, Clone)]
pub enum WsCommand {
    /// Envia um frame de texto.
    Send(String),
    /// Fecha a conexão (frame de close + encerra o loop).
    Close,
}

/// Um evento de um stream de vida longa, emitido pela tarefa de rede e
/// convertido pelo motor em [`crate::EngineMessage::LuauStream`]. Precisa ser
/// `Clone`/`Debug` porque viaja dentro do `EngineMessage`.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// WebSocket pronto: leva o canal para o motor enviar comandos de saída.
    /// Só ocorre em WebSocket (SSE é somente leitura); precede o `Open`.
    Ready(WsSender),
    /// Conexão estabelecida.
    Open,
    /// Uma mensagem/evento chegou (texto).
    Message(String),
    /// Erro na conexão (mensagem).
    Error(String),
    /// Conexão encerrada.
    Closed,
}

/// Identidade + parâmetros de um stream, usados por `Subscription::run_with`.
/// Deriva `Hash` porque o iced identifica (e deduplica/mantém viva) cada
/// subscription pela sua chave. Ver [`crate::GlacierUI::subscription`].
#[derive(Debug, Clone, Hash)]
pub struct StreamKey {
    /// Identidade do motor dono do stream. Duas janelas rodando o mesmo
    /// componente têm motores distintos (`engine_id` distintos), então incluí-lo
    /// na chave impede que o iced funda os dois streams num só recipe.
    pub engine_id: u64,
    pub owner: String,
    pub id: u64,
    pub kind: StreamKind,
    pub url: String,
    pub headers: Vec<(String, String)>,
}

/// Abre um stream **SSE** (Server-Sent Events) sobre HTTP e devolve um
/// [`Stream`] dos [`StreamEvent`]. Reaproveita o cliente hyper+rustls do
/// `fetch`, mas em vez de coletar o corpo inteiro lê frame a frame, acumulando
/// bytes e emitindo um `Message` por evento (`\n\n`), pegando as linhas
/// `data:`. É somente leitura — não há canal de saída (nenhum `Ready`).
pub(crate) fn sse(url: String, headers: Vec<(String, String)>) -> impl Stream<Item = StreamEvent> {
    iced::stream::channel(
        64,
        move |mut output: futures::channel::mpsc::Sender<StreamEvent>| async move {
            install_default_crypto();

            let mut builder = hyper::Request::builder()
                .method(Method::GET)
                .uri(&url)
                .header("accept", "text/event-stream");
            for (k, v) in &headers {
                builder = builder.header(k.as_str(), v.as_str());
            }
            let request = match builder.body(Full::new(Bytes::new())) {
                Ok(r) => r,
                Err(e) => {
                    let _ = output.send(StreamEvent::Error(e.to_string())).await;
                    let _ = output.send(StreamEvent::Closed).await;
                    return;
                }
            };

            let response = match client().request(request).await {
                Ok(r) => r,
                Err(e) => {
                    let _ = output.send(StreamEvent::Error(e.to_string())).await;
                    let _ = output.send(StreamEvent::Closed).await;
                    return;
                }
            };
            let _ = output.send(StreamEvent::Open).await;

            let mut body = response.into_body();
            let mut buf = String::new();
            loop {
                match body.frame().await {
                    Some(Ok(frame)) => {
                        let Some(chunk) = frame.data_ref() else {
                            continue;
                        };
                        // Normaliza CRLF: o parser abaixo trabalha só com '\n'.
                        buf.push_str(&String::from_utf8_lossy(chunk).replace('\r', ""));
                        // Um evento SSE termina numa linha em branco (`\n\n`).
                        while let Some(pos) = buf.find("\n\n") {
                            let raw: String = buf.drain(..pos + 2).collect();
                            if let Some(data) = parse_sse_event(&raw) {
                                let _ = output.send(StreamEvent::Message(data)).await;
                            }
                        }
                    }
                    Some(Err(e)) => {
                        let _ = output.send(StreamEvent::Error(e.to_string())).await;
                        break;
                    }
                    None => break,
                }
            }
            let _ = output.send(StreamEvent::Closed).await;
        },
    )
}

/// Extrai o payload de um bloco de evento SSE: junta com `\n` os valores das
/// linhas `data:` (ignorando `event:`, `id:`, `retry:` e comentários `:`).
/// Devolve `None` se o bloco não tem nenhuma linha `data:`.
fn parse_sse_event(raw: &str) -> Option<String> {
    let mut data: Vec<&str> = Vec::new();
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            // Uma única espaço opcional após os dois-pontos faz parte do formato.
            data.push(rest.strip_prefix(' ').unwrap_or(rest));
        }
    }
    if data.is_empty() {
        None
    } else {
        Some(data.join("\n"))
    }
}

/// Abre um **WebSocket** e devolve um [`Stream`] dos [`StreamEvent`]. Cria um
/// canal interno de comandos e o entrega ao motor no [`StreamEvent::Ready`]
/// (para `conn:send`/`conn:close`); depois faz `select` entre ler frames do
/// socket (viram `Message`) e comandos de saída, até fechar.
pub(crate) fn websocket(
    url: String,
    headers: Vec<(String, String)>,
) -> impl Stream<Item = StreamEvent> {
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};

    iced::stream::channel(
        64,
        move |mut output: futures::channel::mpsc::Sender<StreamEvent>| async move {
            install_default_crypto();

            let mut request = match url.as_str().into_client_request() {
                Ok(r) => r,
                Err(e) => {
                    let _ = output.send(StreamEvent::Error(e.to_string())).await;
                    let _ = output.send(StreamEvent::Closed).await;
                    return;
                }
            };
            for (k, v) in &headers {
                if let (Ok(name), Ok(val)) = (
                    HeaderName::from_bytes(k.as_bytes()),
                    HeaderValue::from_str(v),
                ) {
                    request.headers_mut().insert(name, val);
                }
            }

            let ws = match connect_async(request).await {
                Ok((ws, _resp)) => ws,
                Err(e) => {
                    let _ = output.send(StreamEvent::Error(e.to_string())).await;
                    let _ = output.send(StreamEvent::Closed).await;
                    return;
                }
            };

            let (mut write, read) = ws.split();
            // Canal de comandos de saída; o motor guarda a ponta `Sender`.
            let (cmd_tx, mut cmd_rx) = futures::channel::mpsc::channel::<WsCommand>(64);
            let _ = output.send(StreamEvent::Ready(cmd_tx)).await;
            let _ = output.send(StreamEvent::Open).await;

            let mut read = read.fuse();
            loop {
                futures::select! {
                    incoming = read.next() => match incoming {
                        Some(Ok(Message::Text(t))) => {
                            let _ = output.send(StreamEvent::Message(t.to_string())).await;
                        }
                        Some(Ok(Message::Binary(b))) => {
                            let text = String::from_utf8_lossy(&b).into_owned();
                            let _ = output.send(StreamEvent::Message(text)).await;
                        }
                        Some(Ok(Message::Close(_))) | None => break,
                        // Ping/Pong/Frame: o tungstenite responde ao ping sozinho.
                        Some(Ok(_)) => {}
                        Some(Err(e)) => {
                            let _ = output.send(StreamEvent::Error(e.to_string())).await;
                            break;
                        }
                    },
                    cmd = cmd_rx.next() => match cmd {
                        Some(WsCommand::Send(t)) => {
                            if write.send(Message::Text(t.into())).await.is_err() {
                                break;
                            }
                        }
                        Some(WsCommand::Close) | None => {
                            let _ = write.close().await;
                            break;
                        }
                    },
                }
            }
            let _ = output.send(StreamEvent::Closed).await;
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sse_junta_linhas_data_e_ignora_o_resto() {
        // Múltiplas linhas `data:` viram um payload multi-linha; `event:`/`id:`
        // e comentários são ignorados.
        let raw = "event: msg\ndata: linha 1\ndata:linha 2\nid: 7\n: comentario\n";
        assert_eq!(parse_sse_event(raw).as_deref(), Some("linha 1\nlinha 2"));
        // Bloco sem `data:` não produz mensagem.
        assert_eq!(parse_sse_event("event: ping\n"), None);
    }

    /// `fetch("file://…")` lê um arquivo local e devolve `ok/200` com o conteúdo
    /// no `body`; um caminho inexistente vira `!ok/404` com a mensagem no `error`.
    #[test]
    fn fetch_file_scheme_le_arquivo_local() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .unwrap();
        let dir = std::env::temp_dir().join(format!("glacier-net-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("conteudo.txt");
        std::fs::write(&file, "olá do disco").unwrap();

        let ok = rt.block_on(perform(PendingFetch::new(
            1,
            format!("file://{}", file.display()),
            "GET".into(),
            None,
            Vec::new(),
        )));
        assert!(ok.ok, "deveria ler: erro={}", ok.error);
        assert_eq!(ok.status, 200);
        assert_eq!(ok.body, "olá do disco");

        let missing = rt.block_on(perform(PendingFetch::new(
            2,
            format!("file://{}", dir.join("nao-existe.txt").display()),
            "GET".into(),
            None,
            Vec::new(),
        )));
        assert!(!missing.ok);
        assert_eq!(missing.status, 404);
        assert!(!missing.error.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Smoke test real de rede (HTTPS via hyper + rustls). Ignorado por padrão
    /// para não depender de rede na CI; rode com:
    /// `cargo test --lib net::tests::https_smoke -- --ignored`.
    #[test]
    #[ignore]
    fn https_smoke() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .unwrap();
        let req = PendingFetch::new(
            1,
            "https://example.com".into(),
            "GET".into(),
            None,
            Vec::new(),
        );
        let res = rt.block_on(perform(req));
        assert!(res.ok, "falhou: status={} erro={}", res.status, res.error);
        assert!(res.body.contains("Example Domain"), "corpo inesperado");
    }

    /// Smoke test real do WebSocket ponta-a-ponta (conecta a um echo público,
    /// envia, recebe de volta, fecha). Ignorado por padrão (depende de rede +
    /// serviço externo); rode com:
    /// `cargo test --lib net::tests::ws_echo_smoke -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn ws_echo_smoke() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut stream = Box::pin(websocket("wss://echo.websocket.org".into(), Vec::new()));
            let mut sender: Option<WsSender> = None;
            let mut got_echo = false;
            while let Some(ev) = stream.next().await {
                match ev {
                    StreamEvent::Ready(s) => sender = Some(s),
                    StreamEvent::Open => {
                        sender
                            .as_mut()
                            .unwrap()
                            .try_send(WsCommand::Send("glacier-ping".into()))
                            .unwrap();
                    }
                    StreamEvent::Message(m) => {
                        // O echo.websocket.org manda uma saudação antes; só o
                        // nosso ping ecoado conta.
                        if m.contains("glacier-ping") {
                            got_echo = true;
                            let _ = sender.as_mut().unwrap().try_send(WsCommand::Close);
                        }
                    }
                    StreamEvent::Error(e) => panic!("ws erro: {e}"),
                    StreamEvent::Closed => break,
                }
            }
            assert!(got_echo, "não recebeu o echo do ping");
        });
    }

    /// SSE ponta-a-ponta contra um servidor HTTP local e hermético (sem rede
    /// externa): sobe um listener que responde `text/event-stream` com dois
    /// eventos e fecha; confere que a leitura frame-a-frame + o parser produzem
    /// exatamente `["hello", "world"]` e depois `Closed`. Exercita o mesmo
    /// caminho de rede do `fetch` (cliente hyper), só que lendo em stream.
    #[test]
    fn sse_le_e_parseia_eventos_de_um_servidor_local() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();

            // Servidor: aceita 1 conexão, manda 2 eventos SSE e fecha (o EOF do
            // `Connection: close` sinaliza o fim do stream).
            let server = tokio::spawn(async move {
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await; // consome o request
                let resp = "HTTP/1.1 200 OK\r\n\
                            Content-Type: text/event-stream\r\n\
                            Connection: close\r\n\r\n\
                            data: hello\n\n\
                            data: world\n\n";
                sock.write_all(resp.as_bytes()).await.unwrap();
                sock.flush().await.unwrap();
            });

            let mut stream = Box::pin(sse(format!("http://{addr}/events"), Vec::new()));
            let mut msgs = Vec::new();
            while let Some(ev) = stream.next().await {
                match ev {
                    StreamEvent::Message(m) => msgs.push(m),
                    StreamEvent::Error(e) => panic!("sse erro: {e}"),
                    StreamEvent::Closed => break,
                    _ => {}
                }
            }
            let _ = server.await;
            assert_eq!(msgs, vec!["hello".to_string(), "world".to_string()]);
        });
    }
}
