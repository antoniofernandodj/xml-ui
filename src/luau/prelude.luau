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

