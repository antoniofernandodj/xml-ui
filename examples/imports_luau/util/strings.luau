-- util/strings.luau
--
-- Módulo de lógica pura (sem rede): utilitários de string. Mostra que dá para
-- modularizar QUALQUER lógica, não só clients — e importar de vários lugares.

local M = {}

--- Remove espaços das pontas.
function M.trim(s)
    return (s:gsub("^%s*(.-)%s*$", "%1"))
end

--- Corta `s` em no máximo `n` chars, colocando "…" se estourar.
function M.truncate(s, n)
    if #s <= n then
        return s
    end
    return s:sub(1, n) .. "…"
end

return M
