-- net/http_client.luau
--
-- Biblioteca reutilizável: um client HTTP que encapsula `base_url`, headers
-- padrão e a chamada de rede. A lógica de rede fica AQUI, isolada; qualquer
-- componente pode `require("net.http_client")` e usar sem repetir código.
--
-- Por rodar no mesmo interpretador do componente, o módulo enxerga o `fetch`
-- do prelúdio — que SUSPENDE a corrotina da ação (async/await), sem travar a
-- UI. Ou seja: `client:get(...)` "espera" a resposta sem bloquear.

local Client = {}
Client.__index = Client

--- Cria um client apontando para `base_url` (ex.: "https://api.exemplo.com").
function Client.new(base_url: string)
    return setmetatable({
        base_url = base_url,
        headers = {},
    }, Client)
end

--- Define um header enviado em toda requisição. Encadeável: `c:set_header(...)`.
function Client:set_header(key: string, val: any)
    self.headers[key] = val
    return self
end

--- GET em `path`. Devolve a tabela `{ ok, status, body, error }` do motor.
function Client:get(path: string)
    return fetch(tostring(self.base_url) .. path, { headers = self.headers })
end

--- POST em `path` com corpo `body` (string). Mesma forma de retorno do `get`.
function Client:post(path: string, body: any)
    return fetch(tostring(self.base_url) .. path, {
        method = "POST",
        body = body,
        headers = self.headers,
    })
end

return Client
