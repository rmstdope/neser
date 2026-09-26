-- Capture one frame headlessly with Mesen2 (any system).
--   CAPTURE_FRAME  frame number to capture (default 120)
--   CAPTURE_OUT    output PNG path (required)
-- Requires "AllowIoOsAccess": true in Mesen2's settings.json.
--
-- The capture runs on startFrame, not endFrame: Mesen2 raises EndFrame before the PPU
-- sends the frame it has just rendered (SnesPpu.cpp and NesPpu.cpp both call
-- ProcessEvent(EndFrame) before SendFrame()), so a screenshot taken there is the
-- previous frame. At the N-th startFrame the screenshot holds frame N, the same frame
-- as NESER's --frames N (nr-mxn).
local target = tonumber(os.getenv("CAPTURE_FRAME") or "120")
local out = os.getenv("CAPTURE_OUT")
local frame = 0
function onStartFrame()
  frame = frame + 1
  if frame == target then
    local f = io.open(out, "wb")
    if f then f:write(emu.takeScreenshot()); f:close(); print("SAVED " .. out)
    else print("ERROR: cannot open " .. tostring(out)) end
    emu.stop(0)
  end
end
emu.addEventCallback(onStartFrame, emu.eventType.startFrame)
