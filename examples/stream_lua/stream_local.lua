-- Aponta para a API Quart local (examples/../python/sse-test):
--   GET  http://127.0.0.1:8000/sse  -> um evento por segundo
--   WS   ws://127.0.0.1:8000/ws     -> saúda ao conectar e ecoa o que enviar

function init()
    ctx.sse_status = "fechado"
    ctx.sse_msg = "(abra o SSE)"
    ctx.ws_status = "desconectado"
    ctx.ws_msg = "(conecte)"
    -- Auto-conecta aos dois streams da API Python assim que o app sobe.
    abrir_sse()
    abrir_ws()
end

-- ---- SSE -----------------------------------------------------------------

function abrir_sse()
    if sse_conn then return end
    ctx.sse_status = "conectando..."
    sse_conn = sse("http://127.0.0.1:8000/sse", {
        on_open    = "sse_aberto",
        on_message = "sse_recebeu",
        on_error   = "sse_erro",
        on_close   = "sse_fechou",
    })
end

function sse_aberto()
    ctx.sse_status = "aberto"
end

function sse_recebeu(data)
    ctx.sse_msg = data
end

function sse_erro(e)
    ctx.sse_status = "erro: " .. e
end

function sse_fechou()
    ctx.sse_status = "fechado"
    sse_conn = nil
end

function fechar_sse()
    if sse_conn then sse_conn:close() end
    sse_conn = nil
    ctx.sse_status = "fechado"
end

-- ---- WebSocket -----------------------------------------------------------

function abrir_ws()
    if ws_conn then
        return
    end
    ctx.ws_status = "conectando..."
    ws_conn = websocket("ws://127.0.0.1:8000/ws", {
        on_open    = "ws_aberto",
        on_message = "ws_recebeu",
        on_error   = "ws_erro",
        on_close   = "ws_fechou",
    })
end

function ws_aberto()
    ctx.ws_status = "conectado"
    -- Envia um ping assim que a conexão abre (prova o caminho de saída).
    ws_conn:send("ola do glacier")
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
    if ws_conn then
        ws_conn:send("ping " .. os.time())
    end
end

function fechar_ws()
    if ws_conn
        then ws_conn:close()
    end
end
