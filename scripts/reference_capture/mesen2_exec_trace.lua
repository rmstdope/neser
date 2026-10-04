-- Trace every instruction between two NMI entries with Mesen2 (NES and SNES), one line each:
--   pc=<address, 6 hex digits> clk=<masterClock before the opcode fetch>
-- NESER's side is `timing_trace exec <rom> --from-nmi A --to-nmi B`; diff the two with
-- scripts/diff_timing_traces.py. NMI entries are counted as mesen2_nmi_clock.lua counts them.
--   TRACE_OUT       output file (required, absolute)
--   TRACE_FROM_NMI  first entry to trace from (required; 0 traces from power-on)
--   TRACE_TO_NMI    entry to stop at, not traced (default TRACE_FROM_NMI + 1)
-- getState() on every instruction is slow (about 250k SNES master clocks a second, nr-4lq):
-- find the window with the NMI-clock log first. Requires "AllowIoOsAccess": true.

if io == nil or os == nil then
  print('ERROR: Lua file access is off; set "AllowIoOsAccess": true in Mesen2\'s settings.json'
    .. " (see scripts/reference_capture/README.md)")
  emu.stop(1)
  return
end

local from = tonumber(os.getenv("TRACE_FROM_NMI") or "")
local to = tonumber(os.getenv("TRACE_TO_NMI") or "") or (from and from + 1)
local out = io.open(os.getenv("TRACE_OUT") or "", "w")
if from == nil or out == nil or to <= from then
  print("ERROR: set TRACE_OUT to a writable absolute path, TRACE_FROM_NMI, and TRACE_TO_NMI above it")
  emu.stop(1)
  return
end

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
local handlerCallback = nil
local tracing = false
-- emu.stop() is not immediate: callbacks keep firing for a while after it, so every one
-- checks this first.
local done = false

local function write(address)
  out:write(string.format("pc=%06X clk=%d\n", address, emu.getState().masterClock))
end

local function finish()
  done = true
  out:close()
  print("SAVED " .. os.getenv("TRACE_OUT"))
  emu.stop(0)
end

-- Is this instruction the first of an NMI handler? Counts the entry if so.
local function isEntry(address)
  if armed and address == handler then
    armed = false
    entries = entries + 1
    return true
  end
  return false
end

local function onAny(address)
  if done then return end
  if isEntry(address) and entries >= to then return finish() end
  write(address)
end

local function startTracing()
  tracing = true
  emu.addMemoryCallback(onAny, emu.callbackType.exec, 0x000000, 0xFFFFFF)
end

-- Until the window opens only the handler is watched; a callback added inside another does
-- not see the instruction that added it, so the window's first line is written here.
local function onHandler(address)
  if done or tracing or not isEntry(address) then return end
  if entries == from then
    write(address)
    startTracing()
  end
end

emu.addEventCallback(function()
  local address = nmiHandler()
  if address ~= handler then
    if handlerCallback ~= nil then
      emu.removeMemoryCallback(handlerCallback, emu.callbackType.exec, handler)
    end
    handler = address
    handlerCallback = emu.addMemoryCallback(onHandler, emu.callbackType.exec, handler)
  end
  armed = true
end, emu.eventType.nmi)

if from == 0 then startTracing() end
