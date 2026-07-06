-- Comportamento do contador, num arquivo Lua separado do template.
-- O template o referencia via <script src="../scripts/contador_externo.luau">.
--
-- O contexto do motor é a tabela global `ctx`: ler/escrever `ctx.contador`
-- atualiza o binding {contador} da markup. Ações de onChange chegam com o
-- texto digitado no 1º argumento e na global `value`.

function init()
    ctx.contador = ctx.contador or 0
    ctx.passo = ctx.passo or 1
end

function incrementar()
    ctx.contador = ctx.contador + ctx.passo
end

function decrementar()
    ctx.contador = ctx.contador - ctx.passo
end

function zerar()
    ctx.contador = 0
end

-- onChange do input: define de quanto em quanto o contador anda.
function definir_passo(v)
    local n = tonumber(v)
    ctx.passo = n or 1
end
