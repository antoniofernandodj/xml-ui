-- Declara e inicializa
_G.__logs = {}

function fetch(url, opts)
    return coroutine.yield({
        __glacier_fetch = true,
        url = url,
        opts = opts or {}
    })
end

function sse(url, opts)
    local id = coroutine.yield({
        __glacier_stream_open = true,
        kind = "sse",
        url = url,
        opts = opts or {}
    })
    return {
        id = id,
        close = function(self)
            return coroutine.yield({
                __glacier_stream_cmd = true,
                id = self.id,
                cmd = "close"
            })
        end,
    }
end

-- Toast (notificação efêmera) a partir do Luau. NÃO suspende: cede um pedido, o
-- motor o exibe (via Context::show_toast) e retoma na hora. `opts` = string
-- (mensagem, kind "info") ou tabela `{ message, kind?, title? }` — kind é
-- "info"|"success"|"warning"|"error".
function toast(opts)
    if type(opts) == "string" then
        opts = { message = opts }
    end
    return coroutine.yield({
        __glacier_toast = true,
        message = opts.message,
        kind = opts.kind,
        title = opts.title,
    })
end

-- Diálogo modal de confirmação. NÃO suspende: cede um pedido, o motor mostra o
-- diálogo (via Context::show_dialog) e retoma na hora. O botão de confirmação
-- despacha `opts.confirm_action` (nome de uma função) como um clique normal; o
-- cancelar só fecha. `opts = { title, message, confirm_label?, confirm_action,
-- cancel_label?, destructive? }`.
function confirm(opts)
    return coroutine.yield({
        __glacier_dialog = true,
        title = opts.title,
        message = opts.message,
        confirm_label = opts.confirm_label,
        confirm_action = opts.confirm_action,
        cancel_label = opts.cancel_label,
        destructive = opts.destructive,
    })
end

-- Navegação. NÃO suspendem: cedem um pedido, o motor troca de tela (via
-- Context::navigate_to/navigate_back) e retoma na hora.
function navigate(screen)
    return coroutine.yield({
        __glacier_nav = true,
        screen = screen,
    })
end

function navigate_back()
    return coroutine.yield({
        __glacier_nav_back = true,
    })
end

-- Abre uma NOVA JANELA. NÃO suspende: cede um pedido, o daemon sobe um motor
-- independente para a janela e retoma na hora. `opts` é uma string (caminho do
-- arquivo) ou uma tabela:
--   open_window("telas/detalhe.gv")
--   open_window({ file = "telas/detalhe.gv", title = "Detalhe", width = 400, height = 300 })
--   open_window({ component = "perfil", title = "Perfil" })  -- componente já registrado
function open_window(opts)
    local t = type(opts) == "table" and opts or { file = opts }
    t.__glacier_window = true
    return coroutine.yield(t)
end

-- Envia uma mensagem para as OUTRAS janelas abertas (não para a própria). NÃO
-- suspende: cede um pedido, o motor o acumula e o daemon o entrega ao
-- `on_broadcast(event, payload)` de cada outra janela, retomando na hora.
-- `payload` é opcional; uma tabela é serializada em JSON automaticamente (a
-- janela receptora a recebe já decodificada de volta numa tabela).
--   broadcast("project_created", { id = "42", name = "api" })
function broadcast(event, payload)
    return coroutine.yield({
        __glacier_broadcast = true,
        event = event,
        payload = payload,
    })
end

-- Fecha a PRÓPRIA janela. NÃO suspende: cede um pedido, o daemon fecha a janela
-- dona deste motor após o turno. Útil logo após um `broadcast(...)` para
-- dispensar uma janela auxiliar (ex.: um formulário) ao concluir.
function close_window()
    return coroutine.yield({
        __glacier_window_close = true,
    })
end

-- Temporizador de disparo único (equivalente a setTimeout). NÃO suspende: cede
-- um pedido, o motor agenda `fn` (função ou nome de função global) para daqui
-- a `ms` milissegundos e retoma na hora devolvendo um handle cancelável.
function after(ms, fn)
    local id = coroutine.yield({
        __glacier_after = true,
        ms = ms,
        fn = fn,
    })
    return {
        id = id,
        cancel = function(self)
            return coroutine.yield({
                __glacier_after_cancel = true,
                id = self.id,
            })
        end,
    }
end

-- Temporizador repetitivo (equivalente a setInterval), construído em cima de
-- `after`: a cada disparo, reagenda a si mesmo, até que `:cancel()` seja
-- chamado no handle devolvido (interrompe só as próximas repetições — o
-- disparo em curso, se já cedido, ainda roda).
function every(ms, fn)
    local invoke = fn
    if type(fn) == "string" then
        invoke = function() _G[fn]() end
    end

    local handle = { cancelled = false }
    local function tick()
        if handle.cancelled then
            return
        end
        invoke()
        if not handle.cancelled then
            handle.timer = after(ms, tick)
        end
    end
    handle.timer = after(ms, tick)
    handle.cancel = function(self)
        self.cancelled = true
        if self.timer then
            self.timer:cancel()
        end
    end
    return handle
end

-- Tamanho atual da janela em px lógicos: `{ width, height }`.
function viewport()
    return { width = __glacier_viewport.width, height = __glacier_viewport.height }
end

function websocket(url, opts)
    local id = coroutine.yield({
        __glacier_stream_open = true,
        kind = "ws",
        url = url,
        opts = opts or {}
    })
    return {
        id = id,
        send = function(self, text)
            return coroutine.yield({
                __glacier_stream_cmd = true,
                id = self.id,
                cmd = "send",
                text = text
            })
        end,
        close = function(self)
            return coroutine.yield({
                __glacier_stream_cmd = true,
                id = self.id,
                cmd = "close"
            })
        end,
    }
end

local Client = {}
Client.__index = Client

function Client.new(base_url: string)
    return setmetatable({
        base_url = base_url,
        headers = {},
    }, Client)
end


function Client:set_header(key: string, val: any)
    self.headers[key] = val
    return self
end

function Client:get(path: string)
    return fetch(tostring(self.base_url) .. path, {
        headers = self.headers
    })
end

function Client:post(path: string, body: any)
    return fetch(tostring(self.base_url) .. path, {
        method = "POST",
        body = body,
        headers = self.headers,
    })
end

