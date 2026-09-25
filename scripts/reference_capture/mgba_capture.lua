-- Capture one frame with mgba-headless (built with ENABLE_SCRIPTING and the
-- framebuffer patch, see the gba-hardware-research skill).
--   CAPTURE_FRAME  frame number to capture (default 120)
--   CAPTURE_OUT    output PNG path (required)
local target = tonumber(os.getenv("CAPTURE_FRAME") or "120")
local out = os.getenv("CAPTURE_OUT")
callbacks:add("frame", function()
  if emu:currentFrame() == target then
    emu:screenshot(out)
    console:log("SAVED " .. out)
    os.exit(0)
  end
end)
