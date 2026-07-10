
-- `require` resolve relativo ao diretório DESTE template (app.gv):
--   net.http_client -> examples/imports_luau/net/http_client.luau
--   util.strings    -> examples/imports_luau/util/strings.luau
-- Os módulos são carregados uma vez; `api` persiste entre as ações.


local http = require("net/http_client");
local strings = require("util/strings");

local api = http.new("https://api.ipify.org")
    :set_header("Accept", "application/json");

function init()
    ctx.status = "pronto";
    ctx.resultado = "(clique no botão)";
end

function buscar()
    ctx.status = "carregando...";
    -- api:get suspende a corrotina por baixo (via fetch) — a UI não trava.
    local res = api:get("?format=json");
    if res.ok then
        ctx.resultado = strings.truncate(strings.trim(res.body), 80);
        ctx.status = "ok (" .. res.status .. ")";
    else
        ctx.resultado = res.error;
        ctx.status = "falhou";
    end
end

function ola_mundo()
    local resultado = json.encode({ ola = "mundo" , greetings = { "ola", "hello" }});
    print(resultado);
end
