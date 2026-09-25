-- Capture one frame headlessly with Mesen2 (any system).
--   CAPTURE_FRAME  frame number to capture (default 120)
--   CAPTURE_OUT    output PNG path (required)
-- Requires "AllowIoOsAccess": true in Mesen2's settings.json.
local target = tonumber(os.getenv("CAPTURE_FRAME") or "120")
local out = os.getenv("CAPTURE_OUT")
local frame = 0
function onEndFrame()
  frame = frame + 1
  if frame == target then
    local f = io.open(out, "wb")
    if f then f:write(emu.takeScreenshot()); f:close(); print("SAVED " .. out)
    else print("ERROR: cannot open " .. tostring(out)) end
    emu.stop(0)
  end
end
emu.addEventCallback(onEndFrame, emu.eventType.endFrame)
