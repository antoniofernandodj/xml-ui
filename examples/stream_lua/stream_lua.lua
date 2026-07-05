-- SSE e WebSocket a partir do Lua. Ao contrário de `fetch` (one-shot), estes
-- são streams de vida longa: `sse`/`websocket` NÃO suspendem — registram o
-- stream e devolvem um handle na hora. Cada evento recebido chama de volta o
-- handler nomeado em `opts` (on_message / on_open / on_error / on_close), que
-- escreve em `ctx` como qualquer ação e a UI reavalia.

function init()
    ctx.sse_status = "fechado"
    ctx.sse_msg = "(abra o SSE)"
    ctx.ws_status = "desconectado"
    ctx.ws_msg = "(conecte)"
end

-- ---- Server-Sent Events (somente leitura) --------------------------------

function abrir_sse()
    if sse_conn then
        return
    end
    ctx.sse_status = "conectando..."
    -- stream de teste que emite um evento por segundo
    sse_conn = sse("https://sse.dev/test", {
        on_open    = "sse_aberto",
        on_message = "sse_recebeu",
        on_error   = "sse_erro",
        on_close   = "sse_fechou",
    })
end

function sse_aberto()  ctx.sse_status = "aberto" end
function sse_recebeu(data) ctx.sse_msg = data end
function sse_erro(e)   ctx.sse_status = "erro: " .. e end
function sse_fechou()
    ctx.sse_status = "fechado"
    sse_conn = nil
end

function fechar_sse()
    if sse_conn then sse_conn:close() end
    sse_conn = nil
    ctx.sse_status = "fechado"
end

-- ---- WebSocket (bidirecional; servidor de echo) --------------------------

function abrir_ws()
    if ws_conn then return end
    ctx.ws_status = "conectando..."
    ws_conn = websocket("wss://echo.websocket.org", {
        on_open    = "ws_aberto",
        on_message = "ws_recebeu",
        on_error   = "ws_erro",
        on_close   = "ws_fechou",
    })
end

function ws_aberto()
    ctx.ws_status = "conectado"
end

function ws_recebeu(data)
    ctx.ws_msg = data
end

function ws_erro(e)
    ctx.ws_status = "erro: " .. e
end

function ws_fechou()
    ctx.ws_status = "desconectado"
    ws_conn = nil
end

function enviar_ws()
    if ws_conn then ws_conn:send("ping " .. os.time()) end
end

function fechar_ws()
    if ws_conn then ws_conn:close() end
end