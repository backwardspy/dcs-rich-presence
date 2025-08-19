local lfs         = require("lfs")

package.path      = package.path .. ";" .. lfs.currentdir() .. "/LuaSocket/?.lua"
package.cpath     = package.cpath .. ";" .. lfs.currentdir() .. "/LuaSocket/?.dll"
local socket      = require("socket")

local conn        = nil

local RETRY_TIME  = 15
local UPDATE_TIME = 15

local function connect()
    local sock = socket.udp()
    sock:setpeername("localhost", 14242)
    return sock
end

local function sendTelemetry(t)
    if not conn then
        conn = connect()
        if not conn then
            return t + RETRY_TIME
        end
    end

    local self = Export.LoGetSelfData()
    if not self then return t + RETRY_TIME end

    local name = Export.LoGetPilotName()
    local vehicle = self.Name
    local ias = Export.LoGetIndicatedAirSpeed()
    local alt_bar = Export.LoGetAltitudeAboveSeaLevel()

    local sent = conn:send(string.format(
        "telem %s,%s,%f,%f,%d",
        name, vehicle, ias, alt_bar, t
    ))
    if not sent then conn = nil end

    return t + UPDATE_TIME
end

local function loadDCSRichPresence()
    local nextT = 0
    local handler = {
        onSimulationStart = function()
            net.log("[DCSRPC] simulation start")
            nextT = sendTelemetry(0)
        end,
        onSimulationStop = function()
            net.log("[DCSRPC] simulation stop")
            nextT = 0
            if conn then
                conn:send("bye")
            end
        end,
        onSimulationFrame = function()
            local t = DCS.getModelTime()
            if t < nextT then return end
            net.log("[DCSRPC] sending telemetry")
            nextT = sendTelemetry(t)
            net.log("[DCSRPC] next send at t=" .. tostring(nextT))
        end
    }
    DCS.setUserCallbacks(handler)
end

local status, err = pcall(loadDCSRichPresence)
if not status then
    net.log("[DCS Rich Presence] load error: " .. tostring(err))
else
    net.log("[DCS Rich Presence] load success")
end
