-- Log the clock at each NMI entry with Mesen2 (NES and SNES), one line per entry:
--   nmi=<n> pc=<handler, 6 hex digits> clk=<masterClock before the handler's first opcode fetch>
-- NESER's side is `timing_trace nmi <rom>`; diff the two with scripts/diff_timing_traces.py.
--   TRACE_OUT   output file (required, absolute)
--   TRACE_NMIS  entries to log (default 3600, a minute of NTSC play)
-- Requires "AllowIoOsAccess": true in Mesen2's settings.json. Why an NMI entry and how it is
-- found: scripts/reference_capture/README.md, "Tracing against Mesen2".

if io == nil or os == nil then
  print('ERROR: Lua file access is off; set "AllowIoOsAccess": true in Mesen2\'s settings.json'
    .. " (see scripts/reference_capture/README.md)")
  emu.stop(1)
  return
end

local wanted = tonumber(os.getenv("TRACE_NMIS") or "3600")
local out = io.open(os.getenv("TRACE_OUT") or "", "w")
if out == nil then
  print("ERROR: set TRACE_OUT to a writable absolute path")
  emu.stop(1)
  return
end

-- The handler is read from the vector at every NMI, so a game that remaps the vector bank
-- (NES mappers) is followed. emu.read is a debugger read: it has no side effects, unlike a
-- CPU read of $FFFA, which some mappers watch (MMC5).
local function nmiHandler()
  local state = emu.getState()
  if state["cpu.emulationMode"] == nil then
    return emu.read(0xFFFA, emu.memType.nesMemory) | (emu.read(0xFFFB, emu.memType.nesMemory) << 8)
  end
  local vector = state["cpu.emulationMode"] and 0xFFFA or 0xFFEA
  return emu.read(vector, emu.memType.snesMemory) | (emu.read(vector + 1, emu.memType.snesMemory) << 8)
end

local entries = 0
local armed = false
local handler = nil
local callback = nil

-- An exec callback fires once per instruction, at the opcode fetch, and getState() then
-- reports the clock before that fetch: the same point NESER samples. It is registered without
-- a cpuType argument; with one, memory callbacks never fire (nr-dh7).
local function onHandler(address)
  if not armed then return end
  armed = false
  entries = entries + 1
  out:write(string.format("nmi=%d pc=%06X clk=%d\n", entries, address, emu.getState().masterClock))
  if entries >= wanted then
    out:close()
    print("SAVED " .. os.getenv("TRACE_OUT"))
    emu.stop(0)
  end
end

-- The nmi event fires when the NMI is raised, a few cycles before the CPU enters the
-- handler, so it arms the handler's exec callback rather than logging itself.
emu.addEventCallback(function()
  local address = nmiHandler()
  if address ~= handler then
    if callback ~= nil then emu.removeMemoryCallback(callback, emu.callbackType.exec, handler) end
    handler = address
    callback = emu.addMemoryCallback(onHandler, emu.callbackType.exec, handler)
  end
  armed = true
end, emu.eventType.nmi)
