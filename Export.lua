local lfs         = require("lfs")

package.path      = package.path .. ";" .. lfs.currentdir() .. "/LuaSocket/?.lua"
package.cpath     = package.cpath .. ";" .. lfs.currentdir() .. "/LuaSocket/?.dll"
local socket      = require("socket")

local conn        = nil

local RETRY_TIME  = 15
local UPDATE_TIME = 15

local function connect()
    local sock = socket.connect("localhost", 14242)
    if sock then sock:setoption("tcp-nodelay", true) end
    return sock
end

function LuaExportStart() end

function LuaExportStop()
    if conn then
        conn:send("bye\n")
        conn:close()
    end
end

function LuaExportActivityNextEvent(t)
    -- local t = LoGetModelTime()

    if not conn then
        conn = connect()
        if not conn then
            return t + RETRY_TIME
        end
    end

    local self = LoGetSelfData()
    if not self then return t + RETRY_TIME end

    local name = LoGetPilotName()
    local vehicle = self.Name
    local ias = LoGetIndicatedAirSpeed()
    local alt_bar = LoGetAltitudeAboveSeaLevel()

    local sent = conn:send(string.format(
        "telem %s,%s,%f,%f,%d\n",
        name, vehicle, ias, alt_bar, t
    ))
    if not sent then conn = nil end

    return t + UPDATE_TIME
end
