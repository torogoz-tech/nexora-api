local storage = require("infrastructure.storage")
function total(order) return storage.save(order) end
