"""Unit tests for the nescartdb.com HTML scraper."""

import unittest

from scripts.nes_rom_db_scraper.nescartdb import NesCartDb, BeautifulSoup
from scripts.nes_rom_db_scraper.rom_database import RomDbKey, HardwareType


@unittest.skipIf(BeautifulSoup is None, "BeautifulSoup4 is required for nescartdb tests")
class TestNesCartDb(unittest.TestCase):
    """Tests for NesCartDb HTML parsing and type coercion."""

    def _static_html(self) -> str:
        return """
<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01 Transitional//EN" "http://www.w3.org/TR/html4/loose.dtd">
<html>
  <head>
    <script async src="https://www.googletagmanager.com/gtag/js?id=UA-179299143-1"></script>
    <script>
     window.dataLayer = window.dataLayer || [];
     function gtag(){dataLayer.push(arguments);}
     gtag('js', new Date());
    gtag('config', 'UA-179299143-1');
    </script>
    <title>Super Mario Bros. - NesCartDB</title>
    <meta http-equiv="Content-Type" content="text/html; charset=utf-8">
    <link href="/css/main.css?20201004" rel="stylesheet" type="text/css">
    <link href="/css/tooltip.css" rel="stylesheet" type="text/css">
    <script type="text/javascript" src="/js/nesdb.js?20200930"></script>
    <script type="text/javascript" src="/js/tooltip.js"></script>
  </head>
  <body onload="detectBrowser()"><a href="https://nescartdb.com/cdn-cgi/content?id=VfalmQ3yDJvHrnjch1X_Xo30PhOSMQ34ev5tBPd6JqE-1770469894.7999613-1.0.1.1-benmLTMQuur_jluwwRXnNZ94pWnTP2JEeS5HzEdLFig" aria-hidden="true" rel="nofollow noopener" style="display: none !important; visibility: hidden !important"></a>
    <a name="top"></a>
    <div class="header">
      <table class="header">
        <tr>
          <td width="100%">[<a href="/">Home</a>] [<a href="/search">Search</a>] <!-- [<a href="software.php">Software</a>] -->[<a href="/guides" title="Work in Progress">Guides</a>] [<a href="/plugins" title="Download CopyNES plugins">Plugins</a>] <!--[<a href="xml.php" title="Download the most recent XML">XML</a>] -->[<a href="/missing" title="View games missing from database">Missing</a>] <!--[<a href="pcb.php" title="View PCB info and stats">PCBs</a>] [<a href="stats.php" title="View various real-time stats">Stats</a>] [<a href="contrib.php">Contributors</a>] --></td>
          <noscript>
            <td nowrap><img src="/img/pending.gif">&nbsp;You need JavaScript enabled (and cookies) to properly use this site!</td>
          </noscript>
          <td nowrap><!-- User: Guest [<a href="popup_login.php" onclick="return popup(this, 'Login')">Login</a>] [<a href="newuser.php">Register</a>] --></td>
        </tr>
      </table>
    </div>
    <br>
    <div class="sidebar">
      <table class="sidebox">
        <tr class="header">
          <td nowrap>Browse Database</td>
        </tr>
        <tr>
          <td>
            <table class="sideboxcontent">
              <tr class="textsmall">
                <td>
                  <table width="100%" cellspacing="0" cellpadding="0">
                    <tr>
                      <td align="center">
                        <div class="textsmall">|<a href="/search/browse/@"> # </a>|<a href="/search/browse/a"> A </a>|<a href="/search/browse/b"> B </a>|<a href="/search/browse/c"> C </a>|<a href="/search/browse/d"> D </a>|<a href="/search/browse/e"> E </a>|<a href="/search/browse/f"> F </a>|<a href="/search/browse/g"> G </a>|<a href="/search/browse/h"> H </a>|<a href="/search/browse/i"> I </a>|<a href="/search/browse/j"> J </a>|<a href="/search/browse/k"> K </a>|<a href="/search/browse/l"> L </a>|<a href="/search/browse/m"> M </a>|<a href="/search/browse/n"> N </a>|<a href="/search/browse/o"> O </a>|<a href="/search/browse/p"> P </a>|<a href="/search/browse/q"> Q </a>|<a href="/search/browse/r"> R </a>|<a href="/search/browse/s"> S </a>|<a href="/search/browse/t"> T </a>|<a href="/search/browse/u"> U </a>|<a href="/search/browse/v"> V </a>|<a href="/search/browse/w"> W </a>|<a href="/search/browse/x"> X </a>|<a href="/search/browse/y"> Y </a>|<a href="/search/browse/z"> Z </a>|<a href="/search/random"> Random Game </a>|</div>
                      </td>
                    </tr>
                  </table>
                </td>
              </tr>
            </table>
          </td>
        </tr>
      </table>
      <br>
      <table class="sidebox">
        <tr class="header">
          <td nowrap>Quick Search</td>
        </tr>
        <tr>
          <td>
            <table class="sideboxcontent">
              <tr class="textsmall">
                <td>
                  <form action="/search/basic" method="get">
                    <table width="100%" cellspacing="0" cellpadding="1">
                      <tr>
                        <td nowrap>
                          <input type="text" name="keywords" style="height: 16px; width: 100px;">
                        </td>
                        <td>
                          <input class="button" type="submit" value="Go">
                        </td>
                      </tr>
                      <tr>
                        <td colspan="2">
                          <select name="kwtype" style="width: 144px;">
                            <option value="game">Titles, Catalog ID</option>
                            <option value="pcb">PCB, Class, Mapper</option>
                            <option value="chip">Part#, Chip Type</option>
                          </select>
                        </td>
                      </tr>
                      <tr>
                        <td class="textsmall" colspan="2">&raquo; <a href="/search">Advanced Search</a></td>
                      </tr>
                    </table>
                  </form>
                </td>
              </tr>
            </table>
          </td>
        </tr>
      </table>
      <br>
      <table class="sidebox">
        <tr class="header">
          <td nowrap><a href="search.php?group=cartid&amp;field=41&amp;order=desc&amp;rows=25" title="Search for the latest profiles">Latest Dumps</a></td>
        </tr>
        <tr>
          <td>
            <table class="sideboxcontent">
              <tr class="textsmall">
                <td><!--
                  <table width="100%" border="0" cellspacing="0" cellpadding="0">
                    <tr>
                      <td class="datetime">01/05/2020 06:22 PM</td>
                    </tr>
                    <tr>
                      <td width="100%" class="textsmall"><a href="profile.php?id=4771">Hello Kitty World</a></td>
                      <td><img src="/img/flag_JAP.gif" title="Japan" alt="Japan" width="22" height="16" border="0"></td>
                    </tr>
                  </table>
                  <hr>
                  <table width="100%" border="0" cellspacing="0" cellpadding="0">
                    <tr>
                      <td class="datetime">01/16/2019 07:07 PM</td>
                    </tr>
                    <tr>
                      <td width="100%" class="textsmall"><a href="profile.php?id=4770">Zenbei!! Pro Basket</a></td>
                      <td><img src="/img/flag_JAP.gif" title="Japan" alt="Japan" width="22" height="16" border="0"></td>
                    </tr>
                  </table>
                  <hr>
                  <table width="100%" border="0" cellspacing="0" cellpadding="0">
                    <tr>
                      <td class="datetime">01/16/2019 06:56 PM</td>
                    </tr>
                    <tr>
                      <td width="100%" class="textsmall"><a href="profile.php?id=4769">Monopoly</a></td>
                      <td><img src="/img/flag_JAP.gif" title="Japan" alt="Japan" width="22" height="16" border="0"></td>
                    </tr>
                  </table>
                  <hr>
                  <table width="100%" border="0" cellspacing="0" cellpadding="0">
                    <tr>
                      <td class="datetime">01/16/2019 06:50 PM</td>
                    </tr>
                    <tr>
                      <td width="100%" class="textsmall"><a href="profile.php?id=4768">Mitsume ga Tooru</a></td>
                      <td><img src="/img/flag_JAP.gif" title="Japan" alt="Japan" width="22" height="16" border="0"></td>
                    </tr>
                  </table>
                  <hr>
                  <table width="100%" border="0" cellspacing="0" cellpadding="0">
                    <tr>
                      <td class="datetime">01/15/2019 09:33 PM</td>
                    </tr>
                    <tr>
                      <td width="100%" class="textsmall"><a href="profile.php?id=4767">Double Dragon II: The Revenge</a></td>
                      <td><img src="/img/flag_JAP.gif" title="Japan" alt="Japan" width="22" height="16" border="0"></td>
                    </tr>
                  </table> -->
                </td>
              </tr>
            </table>
          </td>
        </tr>
      </table>
      <br>
      <table class="sidebox">
        <tr class="header">
          <td nowrap><a href="stats.php">Database Stats</a></td>
        </tr>
        <tr>
          <td>
            <table class="sideboxcontent">
              <tr class="textsmall">
                <td><!--
                  &raquo; Cart Profiles:&nbsp;<span class="datetime">4597</span><br>
                  &raquo; Unique Games:&nbsp;<span class="datetime">2437</span><br>
                  &raquo; Unique PCBs:&nbsp;<span class="datetime">628</span><br>
                  &raquo; Unique ROMs:&nbsp;<span class="datetime">3623</span><br>
                  &raquo; Unique Chips:&nbsp;<span class="datetime">1015</span><br>
                  &raquo; Scans:&nbsp;<span class="datetime">11330</span><br>
                  -->
                </td>
              </tr>
            </table>
          </td>
        </tr>
      </table>
    </div>
    <div class="content">      <table>
        <tr>
          <td rowspan="2">
            <table align="left">
              <tr>
                <td>
                  <a class="myimg" href="/profile/image/1?position=cart_top" onmouseover="ddrivetip('Cart Top<br><br>Submitted by: bootgod<br>Linked from profile 0')" onMouseout="hideddrivetip()">
                  <img align="middle" border="0" alt="" src="/images/175/a75920a987324c69e51fe6b55609d989.jpg">
                  </a>
                </td>
              </tr>
            </table>
          </td>
          <td nowrap class="headingmain" valign="bottom" width="100%">
            Super Mario Bros.          </td>
          <td rowspan="2" align="right"><!--
            <table class="simbox">
              <tr class="header">
                <td nowrap>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;Operations&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;</td>
              </tr>
              <tr>
                <td>
                  <table class="simboxcontent">
                    <tr class="textsmall">
                      <td>
                        <a href="?cartid=2570" onclick="return popup(this, 'AddImage')"><img src="/img/image_add.png" title="Add an image to profile" alt="add image" border="0"></a><a href="?cartid=2570" onclick="return popup(this, 'RemoveImage')"><img src="/img/image_delete.png" title="Remove an image from profile" alt="remove image" border="0"></a><a href="?cartid=2570" onclick="return popup(this, 'ReportError')"><img src="/img/error.png" title="Report an error with this profile" alt="report error" border="0"></a><a href="?cartid=2570" onclick="return popup(this, 'DisableProfile')"><img src="/img/delete.png" title="Disable this profile" alt="disable" border="0"></a><a href="?cartid=2570" onclick="return popup(this, 'GroupProfile')"><img src="/img/unlink.png" title="Remove relationship to other profiles" alt="group" border="0"></a><a href="?cartid=2570" onclick="return popup(this, 'DuplicateProfile')"><img src="/img/page_copy.png" alt="duplicate" title="Create a duplicate of this profile" border="0"></a>
                      </td>
                    </tr>
                  </table>
                </td>
              </tr>
            </table> -->
          </td>
          <td rowspan="2" align="right">
            <table class="simbox">
              <tr class="header">
                <td nowrap>&nbsp;More Profiles&nbsp;</td>
              </tr>
              <tr>
                <td>
                  <table class="simboxcontent">
                    <tr class="textsmall">
                      <td>
                        <img src="/img/flag_usa.gif" title="USA (USA): Super Mario Bros." alt="USA (USA): Super Mario Bros." width="22" height="16" border="0">
                      </td>
                      <td align="right" width="100%"><a href="/profile/view/270">1</a>|<a href="/profile/view/1"><u>2</u></a>|<a href="/profile/view/823">3</a>|<a href="/profile/view/836">4</a>|<a href="/profile/view/3063">5</a>                      </td>
                    </tr>
                  </table>
                </td>
              </tr>
            </table>
          </td>
          <td rowspan="2" align="right">
            <table class="simbox">
              <tr class="header">
                <td nowrap>&nbsp;Related Profiles&nbsp;</td>
              </tr>
              <tr>
                <td>
                  <table class="simboxcontent">
                    <tr class="textsmall">
                      <td>
                        <a href="/profile/view/2082/super-mario-bros"><img src="/img/flag_eec.gif" title="Europe (EEC): Super Mario Bros." alt="flag_eec.gif" title="Super Mario Bros." width="22" height="16" border="0"></a>&nbsp;<a href="/profile/view/2122/super-mario-bros"><img src="/img/flag_ger.gif" title="Germany (NOE): Super Mario Bros." alt="flag_ger.gif" title="Super Mario Bros." width="22" height="16" border="0"></a>&nbsp;<a href="/profile/view/1486/super-mario-bros"><img src="/img/flag_jap.gif" title="Japan: Super Mario Bros." alt="flag_jap.gif" title="Super Mario Bros." width="22" height="16" border="0"></a>&nbsp;<a href="/profile/view/2357/super-mario-bros"><img src="/img/flag_scn.gif" title="Scandinavia (SCN): Super Mario Bros." alt="flag_scn.gif" title="Super Mario Bros." width="22" height="16" border="0"></a>&nbsp;<a href="/profile/view/2010/super-mario-bros"><img src="/img/flag_esp.gif" title="Spain (ESP): Super Mario Bros." alt="flag_esp.gif" title="Super Mario Bros." width="22" height="16" border="0"></a>&nbsp;<a href="/profile/view/2459/super-mario-bros"><img src="/img/flag_gbr.gif" title="United Kingdom (GBR): Super Mario Bros." alt="flag_gbr.gif" title="Super Mario Bros." width="22" height="16" border="0"></a>&nbsp;                      </td>
                    </tr>
                  </table>
                </td>
              </tr>
            </table>
          </td>
        </tr>
        <tr valign="top">
          <td nowrap class="textsmall">
            &raquo; Submitted by: <a href="/search/advanced?sub=bootgod&group=cartid">bootgod</a> &nbsp;&nbsp;Time: <span class="datetime">2005-09-22 17:05:00</span>    
          </td>
        </tr>
        <tr >
          <td colspan="5">
            <img src="/img/head_line.gif" alt="" class="line">    
          </td>
        </tr>
      </table>
      <table>
        <tr valign="top">
          <td>
            <table><tr><td height="230"></td></tr></table>
          </td>
          <td >
            <table >
              <tr valign="top">
                <td id="cartfront" width="200">
                  <table align="left">
                    <tr>
                      <td>
                        <a href="/profile/image/1?position=cart_front" class="myimg" onMouseover="ddrivetip('Cart Front<br>Submitted by: bootgod<br>Linked from profile 0')" onMouseout="hideddrivetip()">
                        <img align="middle" border="0" alt="" src="/images/175/dee4bcf1036b7d2cd2d10c70b4ed3cfe.jpg">
                        </a>
                      </td>
                    </tr>
                  </table>
                </td>
                <td id="cartback" style="display:none;" width="200">
                  <table align="left">
                    <tr >
                      <td >
                        <a href="/profile/image/1?position=cart_back" class="myimg" onMouseover="ddrivetip('Cart Front<br>Submitted by: Kinopio<br>Linked from profile 2417')" onMouseout="hideddrivetip()">
                        <img align="middle" border="0" alt="" src="/images/175/bff9570edd616c042fb15caad2d60275.jpg">
                        </a>
                      </td>
                    </tr>
                  </table>
                </td>
              </tr>
              <tr >
                <td align="center">
                  <button id="cartpicbutton" name="cartback" type="button" onclick="swap_cart_pic()">View Other Side</button>
                </td>
              </tr>
            </table>
          </td>
          <td>
            <table>
              <tr class="textmain" align="left">
                <th >Catalog ID</th>
                <td class="textmain" width="100%">NES-SM-USA</td>
              </tr>
              <tr class="textmain" align="left">
                <th >Region</th>
                <td class="textmain" width="100%"><img src="/img/flag_usa.gif" title="USA" alt="USA" width="22" height="16" border="0">&nbsp;USA (NTSC)</td>
              </tr>
              <tr class="textmain" align="left">
                <th >Class</th>
                <td class="textmain" width="100%">Licensed</td>
              </tr>
              <tr class="textmain" align="left">
                <th >Release Date</th>
                <td class="textmain" width="100%">October 18, 1985</td>
              </tr>
              <tr class="textmain" align="left">
                <th >Publisher</th>
                <td class="textmain" width="100%"><a href="/search/advanced?publisher=Nintendo">Nintendo</a></td>
              </tr>
              <tr class="textmain" align="left">
                <th >Developer</th>
                <td class="textmain" width="100%"><a href="/search/advanced?developer=Nintendo">Nintendo</a></td>
              </tr>
              <tr class="textmain" align="left">
                <th >Players</th>
                <td class="textmain" width="100%">2</td>
              </tr>
              <tr class="textmain" align="left">
                <th >Peripherals</th>
                <td class="textmain" width="100%"><!-- <img src="ctrl_pad.png" alt="NES Controller" title="NES Controller"  border="0">&nbsp; -->NES Controller</td>
              </tr>
              <tr >
                <td ><img src="spacer.gif" alt="" width="120" height="1" border="0"></td>
                <td ><img src="spacer.gif" alt="" width="180" height="1" border="0"></td>
              </tr>
            </table>
          </td>
          <td >
            <table >
              <tr align="left">
                <td colspan="2" class="headingmain">Cart Properties</td>
              </tr>
              <tr >
                <td colspan="2"><img src="/img/head_line.gif" alt="" width="100%" height="1" vspace="2"></td>
              </tr>
              <tr class="textmain" align="left">
                <th >Cart Producer</th>
                <td class="textmain" width="100%"><img src="/img/prod_nintendo.png" border="0" vspace="0" alt="" title="Nintendo (NES)"></td>
              </tr>
              <tr class="textmain" align="left">
                <th >Color</th>
                <td class="textmain" width="100%">Grey</td>
              </tr>
              <tr class="textmain" align="left">
                <th >Form Factor</th>
                <td class="textmain" width="100%">3-Screw</td>
              </tr>
              <tr class="textmain" align="left">
                <th >Embossed Text</th>
                <td class="textmain" width="100%">Nintendo&reg; Pat. Pend. Made in Japan</td>
              </tr>
              <tr class="textmain" align="left">
                <th >Front Label ID</th>
                <td class="textmain" width="100%">Not Present</td>
              </tr>
              <tr class="textmain" align="left">
                <th >Seal of Quality</th>
                <td class="textmain" width="100%">Round</td>
              </tr>
              <tr class="textmain" align="left">
                <th >MfgString Present</th>
                <td class="textmain" width="100%">Yes</td>
              </tr>
              <tr class="textmain" align="left">
                <th >Back Label ID / Desc</th>
                <td class="textmain" width="100%">Grey</td>
              </tr>
              <tr class="textmain" align="left">
                <th >2-digit Code</th>
                <td class="textmain" width="100%">00</td>
              </tr>
<!--
              <tr class="textmain" align="left">
                <th >Cart Producer</th>
                <td class="textmain" width="100%"><img src="/img/prod_tengen.png" border="0" vspace="0" alt="" title="Tengen"></td>
              </tr>
-->
              <tr>
                <td ><img src="spacer.gif" alt="" width="150" height="1" border="0"></td>
              </tr>
            </table>
          </td>
        </tr>
      </table>
      <table>
        <tr valign="top">
          <td width="450">
            <table>
              <tr align="left">
                <td colspan="4" class="headingmain">ROM Details</td>
              </tr>
              <tr class="textmain" align="left">
                <th width="40">Type</th>
                <th width="200">Label</th>
                <th width="50" align="center">Size</th>
                <th width="80" align="center">CRC32</th>
                <th width="20" title="Times verified">x !</th>
              </tr>
              <tr>
                <td colspan="5"><img src="head_line.gif" alt="" width="100%" height="1" vspace="1" border="0"></td>
              </tr>
              <tr class="textmain">
                <td>PRG0</td>
                <td>HVC-SM-0 PRG &nbsp;</td>
                <td align="center">32 KB</td>
                <td align="center" title="SHA-1:FEFA1097449A3A11EBF8C6199E905996C5DC8FBD">5CF548D3</td>
                <td align="center">15</td>
              </tr>
              <tr class="textmain">
                <td>CHR0</td>
                <td>HVC-SM-0 CHR &nbsp;</td>
                <td align="center">8 KB</td>
                <td align="center" title="SHA-1:394BADAF0B0BDD0EA279A1BCA89A9D9DDC00B1B5">867B51AD</td>
                <td align="center">27</td>
              </tr>
              <tr>
                <td colspan="5"><img src="head_line.gif" alt="" width="100%" height="1" vspace="1" border="0"></td>
              </tr>
              <tr class="textmain">
                <td colspan="2">ROMs Combined:</td>
                <td align="center">40 KB</td>
                <td align="center" title="SHA-1:FACEE9C577A5262DBE33AC4930BB0B58C8C037F7">D445F698</td>
                <td align="center">17</td>
              </tr>

              <tr>
                <td colspan="5">
                  <table>
                    <tr>
                      <td>
                        <table align="left">
                          <tr>
                            <td>
                              <a href="/profile/image/1?position=pcb_front" class="myimg" onMouseover="ddrivetip('PCB Front<br>NES-NROM-256-03<br>Submitted by: bootgod<br>Linked from profile 0')" onMouseout="hideddrivetip()">
                              <img align="middle" border="0" alt="" src="/images/175/2e9e81224b2320deb4d51d8a464f8c5d.jpg">
                              </a>
                            </td>
                          </tr>
                        </table>
                      </td>
                      <td >
                        <table align="left">
                          <tr>
                            <td>
                              <a href="/profile/image/1?position=pcb_back" class="myimg" onMouseover="ddrivetip('PCB Back<br>NES-NROM-256-03<br>Submitted by: Kinopio<br>Linked from profile 2055')" onMouseout="hideddrivetip()">
                              <img align="middle" border="0" alt="" src="/images/175/6d26509249f161184d69d98ece2cba53.jpg">
                              </a>
                            </td>
                          </tr>
                        </table>
                      </td>
                    </tr>
                  </table>
                </td>
              </tr>
            </table>
          </td>
          <td width="77"></td>
          <td>
            <table>
              <tr align="left">
                <td colspan="2" class="headingmain"><a href="/search/advanced?pcb=NES-NROM-256-03">NES-NROM-256-03</a></td>
              </tr>
              <tr>
                <td colspan="2"><img src="head_line.gif" width="100%" alt="" height="1" vspace="5" border="0"></td>
              </tr>
              <tr class="textmain" align="left">
                <th>PCB Producer</th>
                <td class="textmain" width="100%"><img src="/img/prod_nintendo.png" border="0" vspace="0" alt="" title="Nintendo"></td>
              </tr>
              <tr class="textmain" align="left">
                <th>Manufacturer</th>
                <td class="textna" width="100%">Not Specified</td>
              </tr>
              <tr class="textmain" align="left">
                <th>Mfg Range</th>
                <td class="textmain" width="100%">8638 - 8911 (5)</td>
              </tr>
              <tr class="textmain" align="left">
                <th>Est First Run</th>
                <td class="textmain" width="100%">September 1986</td>
              </tr>
              <tr class="textmain" align="left">
                <th>PCB Class</th>
                <td class="textmain" width="100%"><a href="/search/advanced?unif=NES-NROM-256">NES-NROM-256</a></td>
              </tr>
              <tr class="textmain" align="left">
                <th>iNES Mapper</th>
                <td class="textmain" width="100%"><a href="/search/advanced?ines=0">0</a></td>
              </tr>
              <tr class="textmain" align="left">
                <th>Mirroring</th>
                <td class="textmain" width="100%">Vertical</td>
              </tr>
              <tr class="textmain" align="left">
                <th>Battery present</th>
                <td class="textna" width="100%">No</td>
              </tr>
              <tr class="textmain" align="left">
                <th>WRAM</th>
                <td class="textna" width="100%">0 KB</td>
              </tr>
              <tr class="textmain" align="left">
                <th>VRAM</th>
                <td class="textna" width="100%">0 KB</td>
              </tr>
              <tr class="textmain" align="left">
                <th>CIC Type</th>
                <td class="textmain" width="100%">6113</td>
              </tr>
<!--
              <tr class="textmain" align="left">
                <th >PCB Producer</th>
                <td class="textmain" width="100%"><img src="prod_tengen.png" border="0" vspace="0" alt="" title="Tengen"></td>
              </tr>
              <tr class="textmain" align="left">
                <th >Battery present</th>
                <td class="textna" width="100%">No</td>
              </tr>
              <tr class="textmain" align="left">
                <th >WRAM</th>
                <td class="textna" width="100%">0 KB</td>
              </tr>
              <tr class="textmain" align="left">
                <th >VRAM</th>
                <td class="textna" width="100%">0 KB</td>
              </tr> -->
              <tr>
                <td ><img src="spacer.gif" alt="" width="150" height="1" border="0"></td>
              </tr>
            </table>
          </td>
        </tr>
      </table>
      <table>
        <tr>
          <td colspan="4">
            <table cellpadding="0">
              <tr align="left">
                <td colspan="2" class="headingmain">Detailed Chip Info</td>
              </tr>
              <tr class="textmain" align="left">
                <th >Designation</th>
                <th align="center">Maker</th>
                <th >Part #</th>
                <th >Type</th>
                <th align="center">Package</th>
                <th >DateCode</th>
                <th >&nbsp</th>
                <th align="right" title="Normalized DateCode">Std</th>
                <th align="center">Misc</th>
              </tr>
              <tr >
                <td colspan="8"><img src="head_line.gif" width="100%" alt="" height="1" vspace="2"></td>
              </tr>
              <tr class="textmain">
                <td><I>U1</I> PRG</td>
                <td height="20" align="center">Ricoh</td>
                <td>RP23256E&nbsp;<SPAN class=subdetail>2002</SPAN></td>
                <td>Mask ROM&nbsp;<SPAN class=subdetail>32 KB</SPAN>&nbsp;<SPAN class=subdetail>(200 ns)</SPAN></td>
                <td align="center">DIP-28</td>
                <td class="textna"><SPAN style="COLOR: white">7J5</SPAN> A6</td>
                <td>&nbsp;&raquo;&nbsp;</td>
                <td class="subdetail" align="center">8741</td>
                <td align="center"></td>
              </tr>
              <tr class="textmain">
                <td><I>U2</I> CHR</td>
                <td height="20" align="center">Ricoh</td>
                <td>RP2364E&nbsp;<SPAN class=subdetail>2000</SPAN></td>
                <td>Mask ROM&nbsp;<SPAN class=subdetail>8 KB</SPAN>&nbsp;<SPAN class=subdetail>(200 ns)</SPAN></td>
                <td align="center">DIP-28</td>
                <td class="textna"><SPAN style="COLOR: white">7K1</SPAN> 49</td>
                <td>&nbsp;&raquo;&nbsp;</td>
                <td class="subdetail" align="center">8741</td>
                <td align="center"></td>
              </tr>
              <tr class="textmain">
                <td><I>U3</I> CIC</td>
                <td height="20" align="center">Sharp</td>
                <td>6113</td>
                <td>Nintendo CIC</td>
                <td align="center">DIP-16</td>
                <td class="textna"><SPAN style="COLOR: white">8742</SPAN> A</td>
                <td>&nbsp;&raquo;&nbsp;</td>
                <td class="subdetail" align="center">8742</td>
                <td align="center"></td>
              </tr>
<!--
              <tr class="textmain">
                <td ><i>U1</i> PRG</td>
                <td height="20" align="center"><img src="ami.gif" border="0" vspace="0" alt="" title="AMI"></td>
                <td ><span class="implied">AM231002</span></td>
                <td >Mask ROM&nbsp;<span class="subdetail">128 KB</span></td>
                <td align="center">DIP-28</td>
                <td class="textna"><span style="color: white">8943</span> </td>
                <td >&nbsp;&raquo;&nbsp;</td>
                <td class="subdetail" align="center">8943</td>
                <td align="center"></td>
              </tr> -->
              <tr>
                <td ><img src="spacer.gif" alt="" width="90" height="1" border="0"></td>
                <td ><img src="spacer.gif" alt="" width="90" height="1" border="0"></td>
                <td ><img src="spacer.gif" alt="" width="150" height="1" border="0"></td>
                <td ><img src="spacer.gif" alt="" width="130" height="1" border="0"></td>
                <td ><img src="spacer.gif" alt="" width="80" height="1" border="0"></td>
                <td ><img src="spacer.gif" alt="" width="50" height="1" border="0"></td>
                <td ><img src="spacer.gif" alt="" width="10" height="1" border="0"></td>
                <td ><img src="spacer.gif" alt="" width="10" height="1" border="0"></td>
                <td ><img src="spacer.gif" alt="" width="70" height="1" border="0"></td>
              </tr>
            </table>
          </td>
        </tr>
      </table>
      <br>
      <div class="headingmain">Additional Images</div>
      <table cellpadding="3">
        <tr valign="top">
          <td>
            <table align="left">
              <tr>
                <td>
                  <a href="/profile/image/1?position=box_front" class="myimg" onmouseover="ddrivetip('Box Front<br>NES-SM USA<br>Submitted by: bootgod<br>Linked from profile 836')" onmouseout="hideddrivetip()">
                  <img align="middle" border="0" alt="" src="/images/175/4a52b5a6cf6cd23e43e354f61f28dc7c.jpg">
                  </a>
                </td>
              </tr>
            </table>
          </td>
          <td>
            <table align="left">
              <tr>
                <td>
                  <a href="/profile/image/1?position=box_back" class="myimg" onmouseover="ddrivetip('Box Back<br>NES-SM USA<br>Submitted by: bootgod<br>Linked from profile 836')" onmouseout="hideddrivetip()">
                  <img align="middle" border="0" alt="" src="/images/175/c53006ec17397d163295557c6a90142b.jpg">
                  </a>
                </td>
              </tr>
            </table>
          </td>
          <td>
            <table align="left">
              <tr>
                <td>
                  <a href="/profile/image/1?position=manual" class="myimg" onmouseover="ddrivetip('Manual<br><br>Submitted by: bootgod<br>Linked from profile 3063')" onmouseout="hideddrivetip()">
                  <img align="middle" border="0" alt="" src="/images/175/0c96f5c5e1b3679eb445904800f99fb7.jpg">
                  </a>
                </td>
              </tr>
            </table>
          </td>
        </tr>
      </table>
    </div>
    <br>
    <div class="footer">
      <table class="footer">
        <tr>
          <td width="100%">[<a href="#top">^ Top</a>] [<a href="/">Home</a>]</td>
          <td><a href="net.php"><img src="/img/spacer.gif" width="1" height="1" border="0"></a>&nbsp;&nbsp;</td>
          <td nowrap>&copy; NES Cart Database 2005 - 2026</td>
        </tr>
      </table>
    </div>
  </body>
</html>
"""

    def test_parse_video_system(self) -> None:
        self.assertEqual(NesCartDb._parse_video_system("Japan NTSC"), "NTSC")
        self.assertEqual(NesCartDb._parse_video_system("PAL (Europe)"), "PAL")
        self.assertIsNone(NesCartDb._parse_video_system(None))

    def test_parse_yes_no(self) -> None:
        self.assertEqual(NesCartDb._parse_yes_no("Yes"), 1)
        self.assertEqual(NesCartDb._parse_yes_no("No"), 0)
        self.assertEqual(NesCartDb._parse_yes_no("true"), 1)
        self.assertEqual(NesCartDb._parse_yes_no("false"), 0)
        self.assertIsNone(NesCartDb._parse_yes_no(""))

    def test_build_result_converts_types(self) -> None:
        scraper = NesCartDb(1)
        result = scraper._build_result(1, self._static_html())

        self.assertEqual(result[RomDbKey.NAME.value], "Super Mario Bros.")
        self.assertEqual(result[RomDbKey.CRC.value], "D445F698")
        self.assertEqual(result[RomDbKey.HARDWARE.value], HardwareType.NES_NTSC)
        self.assertEqual(result[RomDbKey.MAPPER.value], 0)
        self.assertEqual(result[RomDbKey.PRG_ROM_SIZE.value], 32768)
        self.assertEqual(result[RomDbKey.CHR_ROM_SIZE.value], 8192)
        self.assertEqual(result[RomDbKey.PRG_RAM_SIZE.value], 0)
        self.assertEqual(result[RomDbKey.CHR_RAM_SIZE.value], 0)
        self.assertIsNone(result.get(RomDbKey.PRG_NVRAM_SIZE.value))
        self.assertEqual(result[RomDbKey.BATTERY.value], 0)
        self.assertEqual(result[RomDbKey.EXPANSION_TYPE.value], 1)
        self.assertNotIn(RomDbKey.SUBMAPPER.value, result)

    def test_build_result_pal_region_sets_nes_pal_hardware(self) -> None:
        """A PAL entry (Region = 'PAL (Europe)') must set hardware to NES_PAL, not NES_NTSC."""
        pal_html = self._static_html().replace(
            "USA (NTSC)", "PAL (Europe)"
        )
        scraper = NesCartDb(1)
        result = scraper._build_result(1, pal_html)
        self.assertEqual(result[RomDbKey.HARDWARE.value], HardwareType.NES_PAL)

    def test_build_result_japan_ntsc_sets_famicom_hardware(self) -> None:
        """A Japan (NTSC) entry must set hardware to FAMICOM, not NES_NTSC."""
        japan_html = self._static_html().replace(
            "USA (NTSC)", "Japan (NTSC)"
        )
        scraper = NesCartDb(1)
        result = scraper._build_result(1, japan_html)
        self.assertEqual(result[RomDbKey.HARDWARE.value], HardwareType.FAMICOM)


if __name__ == "__main__":
    unittest.main()
