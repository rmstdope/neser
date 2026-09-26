-- Capture one frame headlessly with Mesen2 (NES and SNES; the frame and crop logic below
-- was verified for those two consoles only).
--   CAPTURE_FRAME  frame number to capture (default 120)
--   CAPTURE_OUT    output PNG path (required)
-- Requires "AllowIoOsAccess": true in Mesen2's settings.json.
--
-- Which frame (nr-mxn). The capture runs on the N-th startFrame and reads the pixels with
-- emu.getScreenBuffer(), which gives frame N, the same frame as NESER's --frames N:
-- * Not on endFrame: Mesen2 raises EndFrame before the PPU sends the frame it has just
--   rendered (SnesPpu.cpp and NesPpu.cpp call ProcessEvent(EndFrame) before SendFrame()).
-- * Not with emu.takeScreenshot(): it copies the video-decode thread's output buffer, and
--   SendFrame() only signals that thread, so on a loaded machine the decode of frame N
--   may not have run yet and the screenshot is still frame N-1.
-- * emu.getScreenBuffer() runs the video filter over the PPU's own buffer on the
--   emulation thread. At startFrame that buffer still holds frame N on both consoles: the
--   SNES PPU switches buffers at the end of scanline 0, the NES PPU on the
--   pre-render line right after raising StartFrame.
local target = tonumber(os.getenv("CAPTURE_FRAME") or "120")
local out = os.getenv("CAPTURE_OUT")

-- A minimal PNG encoder (8-bit RGB, no filtering, stored deflate blocks): the pixels
-- come from getScreenBuffer() as 0xRRGGBB integers, and Mesen2's Lua has no PNG writer
-- for them.
local crcTable = {}
for n = 0, 255 do
  local c = n
  for _ = 1, 8 do
    if c & 1 == 1 then c = 0xEDB88320 ~ (c >> 1) else c = c >> 1 end
  end
  crcTable[n] = c
end

local function crc32(data)
  local c = 0xFFFFFFFF
  for i = 1, #data do
    c = crcTable[(c ~ data:byte(i)) & 0xFF] ~ (c >> 8)
  end
  return c ~ 0xFFFFFFFF
end

local function adler32(data)
  local a, b = 1, 0
  for i = 1, #data do
    a = (a + data:byte(i)) % 65521
    b = (b + a) % 65521
  end
  return (b << 16) | a
end

local function chunk(kind, data)
  return string.pack(">I4", #data) .. kind .. data .. string.pack(">I4", crc32(kind .. data))
end

local function encodePng(pixels, width, height, firstRow)
  local rows = {}
  for y = firstRow, firstRow + height - 1 do
    local row = { "\0" }
    for x = 1, width do
      local p = pixels[y * width + x]
      row[#row + 1] = string.char((p >> 16) & 0xFF, (p >> 8) & 0xFF, p & 0xFF)
    end
    rows[#rows + 1] = table.concat(row)
  end
  local raw = table.concat(rows)
  local blocks = { "\x78\x01" }
  for pos = 1, #raw, 65535 do
    local piece = raw:sub(pos, pos + 65534)
    local final = (pos + 65535 > #raw) and 1 or 0
    blocks[#blocks + 1] = string.pack("<BI2I2", final, #piece, (~#piece) & 0xFFFF) .. piece
  end
  blocks[#blocks + 1] = string.pack(">I4", adler32(raw))
  return "\x89PNG\r\n\26\n"
    .. chunk("IHDR", string.pack(">I4I4BBBBB", width, height, 8, 2, 0, 0, 0))
    .. chunk("IDAT", table.concat(blocks))
    .. chunk("IEND", "")
end

-- getScreenBuffer() skips the overscan crop that emu.takeScreenshot() applies. The SNES
-- buffer holds 239 lines (478 in hi-res); keep the 224 (448) that NESER outputs, which
-- start 7 (14) lines down, as Mesen2's default SNES overscan of 7 top and 8 bottom does.
-- The NES buffer is the 240 lines NESER outputs, uncropped.
local snesCrop = { [239] = { 7, 224 }, [478] = { 14, 448 } }

local frame = 0
function onStartFrame()
  frame = frame + 1
  if frame == target then
    local pixels = emu.getScreenBuffer()
    local width = emu.getScreenSize().width
    local height = #pixels // width
    local firstRow = 0
    if snesCrop[height] then firstRow, height = snesCrop[height][1], snesCrop[height][2] end
    if (#pixels // width) * width ~= #pixels then
      print("ERROR: " .. #pixels .. " pixels is not a multiple of width " .. width)
    else
      local f = io.open(out, "wb")
      if f then f:write(encodePng(pixels, width, height, firstRow)); f:close(); print("SAVED " .. out)
      else print("ERROR: cannot open " .. tostring(out)) end
    end
    emu.stop(0)
  end
end
emu.addEventCallback(onStartFrame, emu.eventType.startFrame)
