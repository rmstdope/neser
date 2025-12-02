# **Implementering av en Pixel-Perfekt PPU i en NES-Emulator: Cykel-Noggrann Analys och Hårdvarutrogenhet**

Denna rapport utgör en expertanalys av de kritiska tekniska aspekterna som måste beaktas vid konstruktionen av en cykel-exakt (pixel-perfekt) implementation av Nintendos Picture Processing Unit (PPU, modell 2C02/2C07) för emuleringsändamål. Målet är att tillhandahålla en detaljerad guide som adresserar inte bara den logiska funktionen utan även den specifika timing, de interna registerflödena och de kända hårdvarufel som är nödvändiga för att korrekt emulera de mest avancerade rastereffekterna och synkroniseringstricken som används i kommersiella NES-titlar.

## **DEL I: Systemintegration och Exakt Timing – Grunden för Cykelnøjaktighet**

Att uppnå pixel-perfektion är synonymt med att implementera systemets tidsdomäner och deras interaktioner med absolut precision. NES-arkitekturen är beroende av asynkron klockning mellan Central Processing Unit (CPU) och PPU, vilket skapar en rigorös miljö där varje operation är tidsbunden.

### **1.1 NES-Klockdynamiken och PPU/CPU-Förhållandet**

PPU:n arbetar med en högre frekvens än CPU:n. Den minsta tidsenheten i PPU är en PPU-cykel, eller *dot*, som korrelerar med produktionen av en enskild pixel på skärmen. Varje scanline (rad) varar exakt 341 PPU-cykler.  
I det vanligaste systemet, NTSC (North America), är förhållandet mellan PPU- och CPU-klockorna exakt 3:1; tre PPU-cykler utförs för varje enskild CPU-cykel. Detta fasta förhållande förenklar synkroniseringen avsevärt jämfört med PAL-systemet. PAL (Europa) använder istället ett icke-heltalsförhållande om 3.2:1 PPU dots per CPU cycle. Detta fraktionella förhållande innebär att emuleringen måste baseras på den totala PPU-dot-räkningen per ram, snarare än att enkelt dela PPU-cykler med tre för att få CPU-cykler, vilket är avgörande för att bibehålla korrekt systemtiming över tid. NTSC producerar 262 scanlines per ram, medan PAL och DENDY producerar 312\.  
**CPU Catch-Up (Halt/Sync)**  
En av de mest kritiska aspekterna för att hantera rastereffekter är hur CPU-skrivningar till PPU-register (2000–2007) hanteras. PPU-register är minnesmappade, och när CPU:n utför en skrivning, måste emulatoren säkerställa att PPU:n har avancerat till *exakt* den PPU-cykel (dot) där skrivningen faktiskt äger rum i hårdvaran. Om CPU:n får exekvera för långt fram utan att PPU:n körs ikapp, kommer en registeråtkomst som är avsedd att inträffa mitt i en synlig scanline (t.ex. för att ändra scrollposition) att bli försenad, vilket leder till att den avsedda effekten förskjuts till nästa scanline eller blir helt felaktig.  
Den enda pålitliga metoden för att garantera denna exakta synkronisering är att implementera en centraliserad *tick-funktion*. Varje CPU-instruktion måste ha en definierad PPU-cykelkostnad. Innan en ny CPU-instruktion påbörjas, måste PPU-tillståndsmaskinen (inklusive rendering och minneshämtningar) avanceras med det exakta antalet dots som motsvarar den senaste CPU-instruktionens exekveringstid. Detta skapar en hård tidskoppling som är oundviklig för att emulera spel som bygger på pixel-nivå timing.  
Tabell 1.1 visar de fundamentala timingparametrarna:  
Table 1.1: PPU Timing Parametrar (NTSC vs. PAL)

| System | Frames per second | PPU dots per CPU cycle | Scanlines per frame | PPU dots per scanline |
| :---- | :---- | :---- | :---- | :---- |
| NTSC | 60 | 3.0 | 262 | 341 |
| PAL | 50 | 3.2 | 312 | 341 |

### **1.2 Ramstruktur, VBlank och Pre-render Scanline**

En fullständig NES-ram består av 262 scanlines (NTSC). Dessa är indelade i tre huvudsakliga faser: synlig rendering (scanlines 0-239), Vertical Blanking (VBlank), och pre-render scanline.  
**VBlank-perioden** inträffar efter scanline 239\. Vid starten av scanline 241 sätts VBlank-flaggan (2002:7) i PPUSTATUS-registret. Om NMI (Non-Maskable Interrupt) har aktiverats via PPUCTRL (2000), triggas denna interrupt, vilket meddelar CPU:n att den nu befinner sig i en "säker" period för att modifiera grafisk data (VRAM/OAM) utan risk för skärmkorruption. NTSC-system har 20 VBlank scanlines, medan PAL har 70\. Mjukvara förlitar sig på att denna flagga eller NMI utlöses exakt i tid.  
**Pre-Render Scanline (-1 eller 261\)** avslutar VBlank. Denna scanline är icke-synlig, men PPU:n utför ändå samma minnesåtkomster (Nametable, Attribute, Pattern Table fetches) som under en normal renderande scanline. Dess syfte är att ladda skiftregistren och PPU:s interna register för den första synliga scanline (0).  
PPUSTATUS-flaggorna (VBlank och Sprite 0 Hit) rensas båda vid början av pre-render scanline.  
En kritisk hårdvarukarakteristik i NTSC-systemet är **Odd-Frame Skipping**: På udda ramar hoppas den sista PPU-cykeln (dot 340\) på pre-render scanline över. PPU:n hoppar direkt från dot 339 till (0, 0), vilket innebär att den scanline endast har 340 cykler. Denna cykelhoppning används för att kompensera för den fysiska utgångens videosignal och är nödvändig att emulera för att undvika "dot crawl" i stillbilder.

## **DEL II: PPU:s Minnesarkitektur och Adressering**

PPU:s minneshantering och adressering är intimt kopplad till kassetten och dess mappar. Emuleringen måste modellera den 14-bitars adressrymd som spänner över $0000-$3FFF.

### **2.1 PPU Adressrymd och Dynamisk Mappning**

PPU-adressrymden är indelad i mönstertabeller och namntabeller. Adressintervallet $0000-$1FFF är normalt mappat till CHR-ROM eller CHR-RAM, som tillhandahålls av kassetten och hanteras av bank switching-mekanismer. Detta område lagrar de faktiska 8x8 pixelmönstren (Pattern Tables).  
Adressintervallet $2000-$2FFF är tillägnat Nametables och är vanligtvis mappat till de 2 kB interna VRAM som finns på NES-huvudkortet. Dessa 2 kB stöder två samtidiga nametable-skärmar. $3000-$3EFF är en spegling av $2000-$2FFF.  
**Nametable Mirroring** är avgörande. PPU:n använder PPU-adresslinjerna, specifikt A10 och A11, för att välja minneskrets. Kassetten konfigurerar dessa val genom hårdvara:

* **Vertikal spegling:** Speglar namntabeller horisontellt (t.ex. 2000 speglas till 2800, och 2400 speglas till 2C00).  
* **Horisontell spegling:** Speglar namntabeller vertikalt (t.ex. 2000 speglas till 2400, och 2800 speglas till 2C00).  
* **4-Screen Mirroring:** Vissa mappar lägger till ytterligare 2 kB VRAM på kassetten, vilket tillåter fyra unika namntabeller samtidigt genom att låta PPU A11 styra valet mellan konsolens VRAM och kassettens VRAM.

En robust emulering får inte anta en statisk minneskonfiguration. Adressering av $2000-$2FFF måste hanteras dynamiskt av den emulerade mappern, eftersom mappern kontrollerar routingen av PPU:s adresslinjer för att implementera den specifika speglingskonfigurationen som krävs av spelet.  
**Palettminne ($3F00-$3FFF):** Palettdata lagras i 32 bytes RAM (Palette RAM). Detta minne är åtkomligt via 2007\. Palettminnet är tekniskt sett utanför VRAM-adressomfånget (2000-2FFF) men hanteras genom PPU:s dataåtkomst. Entréerna 3F00 och 3F10 (bakgrundsfärg och sprite-bakgrundsfärg) delar samma interna minnesplats.

### **2.2 $2007 (PPUDATA) Läsbuffer och Latens**

Åtkomst till VRAM-minnet via 2007 från CPU:n är inte omedelbar på grund av PPU:s interna buffer. Eftersom CHR-ROM/RAM betraktas som externa enheter till PPU:n, måste PPU:n först hämta data till en intern buffer innan den kan returnera värdet till CPU:n.  
**Dummy Read-mekanismen:** Den första CPU-läsningen från 2007 (om adressen är under 3F00, Nametables eller Pattern Tables) returnerar innehållet i den *tidigare* laddade buffern. Det är först vid den andra, omedelbart efterföljande läsningen, som det faktiska, uppdaterade värdet från den aktuella VRAM-adressen returneras. För palettminnet (3F00 till 3FFF) finns ingen dummy read, och data returneras omedelbart. Efter varje läsning eller skrivning till 2007 inkrementeras den interna VRAM-adressen (v) automatiskt med antingen 1 eller 32, beroende på bit 2 i 2000 (PPUCTRL).  
**2007-åtkomst under Rendering Glitch:** Ett mindre, men kritiskt, hårdvarufel uppstår när CPU:n läser eller skriver till 2007 under den aktiva renderingen (scanlines 0-239). Under denna period orsakar 2007-åtkomsten att PPU:ns VRAM-adress inkrementeras på ett oväntat sätt. Det aktiverar både horisontell (Coarse X) och vertikal (Scanline Y) inkrementering samtidigt, oavsett inställningen i 2000-registret. Denna glitch utnyttjades av spel som *Young Indiana Jones Chronicles* och *Burai Fighter* för att utföra mitt-i-skärmen Y-scrolljusteringar, eftersom vanliga 2005-skrivningar inte tillåter vertikal scrolländring mid-frame.

## **DEL III: De Interna Renderingstillstånden och Scrolling-Logik**

Korrekt implementering av PPU:s interna adressregister, ofta kallade "Loopy"-registren, är grundläggande för pixel-perfektion, särskilt för att hantera scrolling och rastereffekter. Dessa register representerar scroll-tillståndet och VRAM-adressen.

### **3.1 Loopy V & T: Adresshantering**

PPU använder fyra interna register för adressering och scrolling:

1. **v (VRAM Address):** Den aktuella 15-bitars adressen som används för att hämta data under rendering.  
2. **t (Temporary VRAM Address):** En 15-bitars kopia av den önskade scrolladressen, som laddas in av CPU:n via 2005 och 2006\.  
3. **x (Fine X Scroll):** De 3 lägsta bitarna av den horisontella scrollen (pixeloffset inom tile).  
4. **w (Write Toggle):** En 1-bitars flagga som växlar mellan första och andra skrivningen till 2005 eller 2006\.

Både v och t har samma logiska struktur, som representerar en 16x16-tile skärm: 0yyyNNYYYYYXXXXX. Här representerar yyy den *fina Y* (pixeloffset inom 8x8 tile), NN nametable-select (2 bitar), YYYYY den *grova Y* (tile row 0-29), och XXXXX den *grova X* (tile column 0-31).  
CPU manipulerar scrollpositionen genom att skriva till 2005 (PPUSCROLL) och 2006 (PPUADDR). Skrivningar till dessa register manipulerar alltid t och x. Write Toggle w är avgörande. Den rensas genom att läsa 2002 (PPUSTATUS).

* **2005 (PPUSCROLL) Första Skrivning (w=0):** Sätter omedelbart Fine X (x) till de lägsta 3 bitarna i data. De övriga 5 bitarna uppdaterar den grova X-komponenten (XXXXX) i t.  
* **2005 (PPUSCROLL) Andra Skrivning (w=1):** Sätter Fine Y (yyy) till de lägsta 3 bitarna i data, och de övriga 5 bitarna uppdaterar den grova Y-komponenten (YYYYY) i t.

Efter två skrivningar till 2005 återställs w. En fullständig scrollposition inställs traditionellt i NMI-hanteraren genom att skriva till 2005 två gånger och sedan sätta nametable-valet (NN) i 2000\.

### **3.2 Automated Scroll Inkrementering under Rendering**

PPU:n utför automatiska uppdateringar av v-registret under rendering för att avancera till nästa tile och nästa scanline.

* **Horisontell Inkrementering (Under rendering):** PPU:n inkrementerar den grova X-koordinaten (XXXXX) i v upprepade gånger. Detta sker varje gång en tile (4 fetches, 8 dots) har renderats, från dot 8 till dot 256\. Om Coarse X når 31 (tile 31), nollställs den till 0, och PPU växlar den horisontella nametable-biten (NN: bit 0). Inkrementering sker också under pre-fetch-fasen vid dots 328 och 336\.  
* **Vertikal Inkrementering (Dot 256):** Exakt vid dot 256, om rendering är aktiverad, inkrementeras den vertikala positionen i v. Först inkrementeras Fine Y (yyy). Om Fine Y overflowar, inkrementeras Coarse Y (YYYYY). Om Coarse Y overflowar, nollställs den till 0, och den vertikala nametable-biten (NN: bit 1\) växlas.

Dessa automatiska inkrementeringar är kärnan i hur PPU:n renderar en 256x240 skärm genom att sekventiellt läsa namntabeller.  
**Registeråterladdning (Reloads):** Vid slutet av varje scanline återställs v från de tillfälliga värdena i t:

1. **Horisontell Reload (Dot 257):** Vid dot 257 kopieras de horisontella komponenterna (Coarse X och horisontell nametable bit) från t till v. Detta förbereder v för att börja hämta den nya scanlinens data med rätt X-scroll.  
2. **Vertikal Reload (Dot 280-304):** Under pre-render scanline (-1), mellan dots 280 och 304, kopieras de vertikala komponenterna (Fine Y, Coarse Y och vertikal nametable bit) från t till v. Detta fastställer den initiala Y-scrollen för den nya ramen.

Eftersom PPU:ns renderingstillstånd endast återställer de horisontella komponenterna vid dot 257 och de vertikala komponenterna endast vid pre-render scanline (dot 304), kan skrivningar till 2005 mitt i en ram endast effektivt ändra den horisontella scrollen. Den andra skrivningen till 2005, som manipulerar Y-komponenterna i t, kommer att ignoreras av PPU:s renderingscykel, eftersom v endast uppdateras vertikalt i slutet av VBlank. Detta förklarar varför spel som *Super Mario Bros.* använder Sprite Zero Hit för att tidsbestämma en X-scrolländring vid skärmdelningar, men inte kan ändra Y-scroll utan att använda 2007-glitchen.

## **DEL IV: Cykel-Exakt Utförande av Rendering-Pipeline**

Den pixel-perfekta emuleringen kräver att PPU:ns inre databearbetning (pipeline) följs exakt per cykel, då detta dikterar både bakgrunds- och sprite-rendering.

### **4.1 Bakgrunds- och Sprite Pipeline (Dot 1-340)**

Varje scanline (0-239) består av 341 dots. Innan renderingen startar vid dot 1 genomgår PPU:n en serie minneshämtningar och OAM-operationer.  
**Bakgrundshämtning (Cycles 1-256):** Denna fas renderar de synliga 256 pixlarna (32 tiles). PPU:n hämtar data för en tile var 8:e PPU-cykel. Varje minnesåtkomst tar 2 PPU-cykler, vilket innebär att 4 åtkomster (8 cykler) krävs per tile. Sekvensen per tile är:

1. **Nametable byte (2 cykler):** Hämtar tile-index från $2000-$2FFF.  
2. **Attribute table byte (2 cykler):** Hämtar palettdata (2 bitar) från $23C0-$2FFF.  
3. **Pattern table tile low (2 cykler):** Hämtar de låga bitarna (8x1 pixel sliver).  
4. **Pattern table tile high (2 cykler):** Hämtar de höga bitarna (8 bytes högre adress).

Dessa hämtade bytes laddas in i interna 16-bitars skiftregister (ett för låg ordning, ett för hög ordning) som sedan skiftas varje cykel för att mata ut pixeldata. Fine X-registret används för att justera utgången från dessa register för sub-tile scrolling.  
**Garbage Fetches (Cycles 257-260) och Pre-Fetches (Cycles 321-336):** Efter att den sista synliga tilen har renderats (dot 256), börjar PPU:n förbereda nästa scanline.

* **Cycles 257-320** används huvudsakligen för sprite-hämtning (se 4.2). Under denna tid utförs också "garbage nametable fetches".  
* **Cycles 321-336** är den kritiska pre-fetch-fasen där PPU:n hämtar data för de första två tilesen av nästa scanline (8 minnesåtkomster, 16 cykler totalt) för att fylla skiftregistren i tid för dot 1 på nästa rad. Den exakta tidpunkten för dessa fetches är fundamental för att hantera raster timing.

PPU-cykel 0 är en inaktiv cykel där PPU:s adressbuss uppvisar samma CHR-adress som den som senare används vid dot 5\.

## **DEL IV: Sprite Utvärdering och Fetch**

Spritehanteringen är tidsbunden och involverar tre huvudsteg per scanline (0-239).  
**1\. Secondary OAM Initialization (Cycles 1-64):** Sekundär OAM (en intern 32-bytes buffer som rymmer max 8 sprites) rensas till värdet FF. Detta initieras genom att PPU läser från Primary OAM och skriver till Secondary OAM, men med en aktiv signal som tvingar läsningen att returnera FF.  
**2\. Sprite Evaluation (Cycles 65-256):** PPU går igenom Primary OAM (256 bytes) för att identifiera de första åtta spritarna som faller inom det synliga Y-intervallet för den aktuella scanlinen.

* Utvärderingen sker cykel-till-cykel; på udda cykler läses data från Primary OAM, och på jämna cykler skrivs data till Secondary OAM.  
* Om åtta sprites hittas, stoppas skrivningen till Secondary OAM, vilket gör att alla bakomliggande sprites utelämnas. Denna fas inkluderar den brutna logik som kan sätta Sprite Overflow-flaggan (se 5.2).

**3\. Sprite Fetches (Cycles 257-320):** Spritedata för de 8 (eller färre) utvalda spritarna hämtas från Pattern Tables. Varje sprite kräver 8 cykler för hämtning. Denna hämtade data lagras i interna sprite-skiftregister, redo att kombineras med bakgrundspixlarna.  
Tabell 4.1 sammanfattar den kritiska cykel-uppdelningen för en synlig scanline (NTSC):  
Table 4.1: PPU Databearbetning per Scanline (Dots 0-340)

| PPU Cykelintervall (Dots) | Funktion | V-register Aktivitet |
| :---- | :---- | :---- |
| 0 | Idle/Bus Load | V-adress baserad på tidigare hämtning |
| 1-64 | Sekundär OAM rensning/initiering | Standard v-inkrementering (om rendering inaktiverat) |
| 65-256 | Bakgrunds-tile hämtning (32 tiles) & Sprite Evaluation | Horisontell inkrementering av v var 8:e dot |
| 256 | Rendering av sista tile | Vertikal inkrementering av v (Fine Y, Coarse Y, NT) |
| 257 | Horisontell Register Reload | Horisontell del av t kopieras till v |
| 257-320 | Sprite Fetches (Pattern Data) | Garbage fetches / Intern V-adressering |
| 321-336 | Pre-Fetch för nästa scanline (2 tiles) | Horisontell inkrementering av v vid dot 328 & 336 |
| 337-340 | Idle/Dummy fetches | Interna räknare |

## **DEL V: Emulering av Hårdvarufel och Raster-Tricks**

Pixel-perfektion uppnås inte genom att emulera perfekt hårdvara, utan genom att exakt återskapa PPU:ns kända hårdvarufel, vilka ofta utnyttjas som funktioner av spelutvecklare.

### **5.1 Sprite Zero Hit-Detektion**

Sprite Zero Hit-flaggan (2002:6) är den primära mjukvarubaserade metoden för att synkronisera med en specifik scanline (rastereffekt) utan hjälp av avancerade mappar.  
**Definition och Logik:** Flaggan sätts när den första icke-transparenta pixeln från Sprite 0 (objekt i OAM index 0\) ligger över en icke-transparent bakgrundspixel. Denna kombinerade färgdetektering måste ske i realtid under pixel-utgången. Flaggan sätts omedelbart vid det första överlappet i en ram och förblir satt tills den rensas på pre-render scanline.  
**Timing och Användning:** Spel som *Super Mario Bros.* använder Sprite Zero Hit för att tidsbestämma en förändring av X-scrollen mid-frame, vilket skapar en stillastående HUD högst upp på skärmen. Emuleringen måste hantera den potentiella risken för att CPU:n fastnar i en poll-loop i väntan på Sprite Zero Hit, vilket kan leda till en krasch om hiten uteblir. Robusta emulatorer måste implementera en timeout-mekanism (t.ex. att även pollning mot VBlank kan tvinga fram en exit) för att förhindra detta.

### **5.2 Implementation av Sprite Overflow Buggen**

Sprite Overflow-flaggan (2002:5) är designad för att indikera när fler än åtta sprites är synliga på en given scanline. I NES PPU är logiken för att detektera detta dock defekt, vilket skapar en av de mest notoriska hårdvarubuggarna.  
**Den Defekta Logiken:** Under Sprite Evaluation (Cycles 65-256), efter att PPU:n har hittat 8 sprites, fortsätter den att scanna Primary OAM för att se om flaggan ska sättas för en nionde sprite. På grund av ett logikfel inkrementeras OAM-indexet felaktigt: istället för att bara inkrementera sprite-index (n) fortsätter den felaktiga logiken att inkrementera byte-offset inom sprite-rekordet (m).  
**Konsekvens:** PPU:n börjar tolka efterföljande spriters Tile Index, Attribute och X-koordinater som om de vore Y-koordinater, vilket leder till en "diagonal" skanning av OAM-data. Detta resulterar i inkonsekventa beteenden, inklusive falska positiva (overflow utlöses trots \\le 8 sprites) och falska negativa (overflow missas trots \> 8 sprites).  
För att uppnå pixel-perfektion är det absolut nödvändigt att emulera denna brutna utvärderingslogik exakt. Spel använder ibland (eller påverkas oavsiktligt av) denna flagga för timing när Sprite Zero Hit eller mapper IRQ:er inte är tillgängliga.

### **5.3 PPU-Interna Busskonflikter och Åtkomst**

PPU:s interna registeroperationer kan kollidera med CPU-skrivningar. Dessa race conditions kan leda till subtila, men viktiga, tillståndsförändringar.  
En kritisk konflikt uppstår när CPU:n skriver till 2005 eller 2006 samtidigt som PPU:ns interna logik uppdaterar v. Till exempel:

* En 2006 skrivning som sätter en ny scrollplats (kopierar t till v) kan kollidera med den vertikala inkrementeringen av v vid dot 256\.  
* En läsning eller skrivning till 2007 (PPUDATA) kan kollidera med PPU:ns naturliga t \\rightarrow v reload vid dot 257 (horisontell) eller dot 304 (vertikal).

Emuleringen av PPU måste därför modellera PPU-cykeln som den minsta enhet där dessa interna och externa skrivningar hanteras atomärt. En CPU-skrivning som inträffar under en kritisk PPU-cykel måste ha företräde eller samverka med PPU:ns interna logik på det sätt som observerats i hårdvara.

## **DEL VI: Autentisk Videoåtergivning**

Även om de flesta emuleringskärnor fokuserar på timing, definierar den slutliga färgkonverteringen den visuella "pixel-perfekta" upplevelsen.

### **6.1 Färgpalettens Fysik (Luma/Chroma)**

NES PPU genererade en analog kompositvideosignal (NTSC eller PAL) direkt från chippet (2C02/2C07), snarare än att använda standard RGB-färgrymder. PPU:n använder en 6-bitars färgkodning, vilket ger 64 möjliga utgångsfärger.  
Dessa 64 färger är baserade på en uppdelning av Hue (4 bitar, kromatisk fas) och Value (2 bitar, luma/ljusstyrka).  
Paletten kan modifieras ytterligare av 3 *emphasis bits* i PPUMASK-registret (2001). Dessa bitar tillämpas som en global färgmodifierare på hela skärmen genom att skifta luma- och chroma-värdena.  
För modern emulering måste dessa 6-bitars värden konverteras till RGB. Eftersom den ursprungliga färggenereringen var analog och beroende av TV-mottagarens kalibrering och de specifika spänningsnivåerna på PPU-chippet, finns det ingen *enda* korrekt RGB-palett. Expertimplementeringar använder empiriskt härledda paletter (t.ex. NTSC-mättade eller "Unsaturated V6") som baseras på mätningar av faktisk hårdvara för att simulera den ursprungliga looken.

### **6.2 Simulering av NTSC Artifacting**

För att uppnå den maximalt autentiska videoupplevelsen kan det krävas att man simulerar NTSC:s analoga artefakter.  
NTSC-signalen blandade luminans (svartvit information) och krominans (färginformation) på samma bärvåg, vilket ledde till färgblödning och *dot crawl*, där krominansen "spiller över" i luman. Denna effekt manifesteras visuellt genom att vertikala linjer kan se "taggiga" ut eller att färger blandas i komplexa mönster som inte är närvarande i PPU:s rena digitala utgång. Vissa spel utnyttjade denna artefakt för att skapa nyanserade färger eller mönster som inte var direkt programmerbara.  
NES PPU har också en funktion där färgsubbärarfasen alterneras med en tredjedels cykel mellan udda och jämna ramar. När dessa 60 FPS-ramar spelas upp och blandas på en CRT, skapar det en mer stabil färgbild. Att emulera denna fasförskjutning och sedan blanda de resulterande ramarna (composite blending) är nödvändigt för att exakt fånga det visuella intrycket av den ursprungliga hårdvaran.

## **DEL VII: DMA och Synkronisering av CPU-Stopp**

Direct Memory Access (DMA) för Object Attribute Memory (OAM) är en kritisk synkroniseringshändelse som fryser CPU:n, vilket kräver att PPU och APU exekveras under den stulna tiden.

### **DMA Processen och CPU Halt Time**

En skrivning till registret 4014 initierar OAM DMA, vilket är en blocköverföring av 256 bytes (en full OAM) från CPU RAM till PPU:s OAM.  
Denna process **halverar CPU:n** (CPU halt) i exakt 513 eller 514 CPU-cykler. Tiden det tar beror på CPU:ns klockfas vid tidpunkten för skrivningen:

* Om DMA initieras på en **jämn** CPU-cykel, tar processen 513 cykler.  
* Om DMA initieras på en **udda** CPU-cykel, krävs en extra initial uppriktningscykel, vilket resulterar i 514 cykler.

Dessa 513/514 cykler är kritisk tid under vilken PPU:n, i ett NTSC-system, avancerar med cirka 1539 till 1542 PPU dots. Emuleringen måste garantera att PPU:ns tillstånd (inklusive Sprite Evaluation och Bakgrundshämtning) fortsätter att avanceras korrekt under hela DMA-perioden, även om CPU:n står stilla.  
**Kritisk Synkronisering under Halt**  
Eftersom DMA kan initieras när som helst under en ram, kan det kollidera med PPU:ns mest tidskänsliga operationer, såsom Sprite Evaluation (Cycles 65-256). Korrekt emulering av PPU:n under DMA kräver att PPU:ns interna tillståndsmaskin uppdateras varje PPU dot under de 513/514 CPU-cyklerna, vilket säkerställer att sprite-datan laddas in i OAM under den korrekta PPU-fasen.  
Denna DMA-hantering understryker behovet av en fullständig systemcykel-synkronisering som integrerar CPU, PPU och APU. APU:n måste också tillåtas att fortsätta exekvera under DMA-stoppet för att förhindra ljudförskjutningar. Vissa avancerade timing-tricks som involverar APU:ns DMC IRQ (för att skapa en grov scanline-räknare när mappar saknas) blir endast möjliga om PPU, CPU och APU är helt synkroniserade, vilket inkluderar hanteringen av DMA-latens.

## **Slutsatser och Rekommendationer**

Implementeringen av en pixel-perfekt PPU i en NES-emulator är ett komplext ingenjörsprojekt som kräver tillämpning av digital hårdvarumodellering snarare än bara logisk funktionalitet.  
Den grundläggande rekommendationen är att etablera en **global PPU dot-räknare** som den primära källan till sanningen för hela systemets timing. Alla CPU-operationer, registeråtkomster, renderingstillstånd och DMA-händelser måste tidsbestämmas relativt till denna räknare, inklusive den korrekta 3:1 (NTSC) eller 3.2:1 (PAL) klockdelningen.  
För att uppnå full kompatibilitet måste emuleringen fokusera på följande kritiska detaljer:

1. **Exakt Timing och Catch-Up:** Implementera CPU-PPU synkroniseringsmekanismer för att säkerställa att registerändringar, särskilt till 2005 och 2006, utförs vid exakta PPU dots för att stödja rastereffekter.  
2. **Loopy Register Logik:** Modellera de interna v, t, x och w registren och de automatiska horisontella och vertikala uppdateringarna vid dots 256, 257 och 304 exakt som beskrivet.  
3. **Hårdvarufel som Funktioner:** Replikera kritisk defekt logik såsom 2007-läsglitchen under rendering, Dummy Read-mekanismen och den brutna Sprite Overflow-logiken, eftersom dessa beteenden är integrerade i spelmjukvarans förväntningar.  
4. **DMA-Cykelstyrning:** Se till att CPU-haltiden under OAM DMA (513/514 cykler) används för att avancera PPU:ns tillstånd, vilket bibehåller korrekt synkronisering mellan alla systemkomponenter.

En PPU-implementering som noggrant följer dessa cykel-exakta och defekta hårdvarumodeller kommer att vara robust nog att köra de mest krävande titlarna, inklusive de som använder skärmdelningar, flerskärmsscrolling och komplexa VRAM-tricks.

#### **Citerade texter**

1\. PPU rendering \- NESdev Wiki, https://www.nesdev.org/wiki/PPU\_rendering 2\. NES Architecture: PPU and CPU timing \- Everything NESmaker, https://nesmaker.nerdboard.nl/2024/10/14/nes-architecture-ppu-and-cpu-timing/ 3\. CPU, PPU, and APU synchronization : r/EmuDev \- Reddit, https://www.reddit.com/r/EmuDev/comments/dnf9xf/cpu\_ppu\_and\_apu\_synchronization/ 4\. NES System Timing (CPU \+ PPU \+ APU) \- Emulation Online, https://www.emulationonline.com/systems/nes/nes-system-timing/ 5\. PPU Scrolling \- Writing NES Emulator in Rust, https://bugzmanov.github.io/nes\_ebook/chapter\_8.html 6\. PPU programmer reference \- NESdev Wiki, https://www.nesdev.org/wiki/PPU\_programmer\_reference 7\. PPU memory map \- NESdev Wiki, https://www.nesdev.org/wiki/PPU\_memory\_map 8\. Console meets cartridge: Breaking down the architecture of the NES's unique design, https://www.xda-developers.com/console-meets-cartridge-breaking-down-the-architecture-of-the-ness-unique-design/ 9\. Questions about NES programming and architecture \- The NESDev forums, https://forums.nesdev.org/viewtopic.php?t=20685 10\. How was MMC3 4-screen mirroring implemented in NES hardware? \- nesdev.org, https://forums.nesdev.org/viewtopic.php?t=6466 11\. Adding 4screen mirroring to UNROM-512 mapper definition \- nesdev.org, https://forums.nesdev.org/viewtopic.php?t=14725 12\. PPU palettes \- NESdev Wiki, https://www.nesdev.org/wiki/PPU\_palettes 13\. Emulating PPU Registers \- Writing NES Emulator in Rust, https://bugzmanov.github.io/nes\_ebook/chapter\_6\_1.html 14\. What is WRONG with my PPU??? \- The NESDev forums, https://forums.nesdev.org/viewtopic.php?t=3585 15\. Why is the current PPU VRAM address updated at scanline 0? \- The NESDev forums, https://forums.nesdev.org/viewtopic.php?t=10499 16\. PPU scrolling \- NESdev Wiki, https://www.nesdev.org/wiki/PPU\_scrolling 17\. User:Bregalad/Split Scrolling \- NESdev Wiki, https://www.nesdev.org/wiki/User:Bregalad/Split\_Scrolling 18\. \[NES\] I don't quite understand PPU scrolling : r/EmuDev \- Reddit, https://www.reddit.com/r/EmuDev/comments/15ibghb/nes\_i\_dont\_quite\_understand\_ppu\_scrolling/ 19\. 18\. Sprite Zero \- nesdoug, https://nesdoug.com/2018/09/05/18-sprite-zero/ 20\. PPU nametables \- NESdev Wiki, https://www.nesdev.org/wiki/PPU\_nametables 21\. PPU sprite evaluation \- NESdev Wiki, https://www.nesdev.org/wiki/PPU\_sprite\_evaluation 22\. Techniques to detect scanline \- nesdev.org, https://forums.nesdev.org/viewtopic.php?t=17813 23\. Deep Dive: intro to sprite zero hit detection \- Everything NESmaker, https://nesmaker.nerdboard.nl/2025/06/10/intro-to-sprite-zero-hit-detection/ 24\. Sprite Overflow Flag: Useful for practical NES programming? \- nesdev.org, https://forums.nesdev.org/viewtopic.php?t=8940 25\. PPU glitches \- NESdev Wiki, https://www.nesdev.org/wiki/PPU\_glitches 26\. PPU memory cycle timing diagram? \- nesdev.org, https://forums.nesdev.org/viewtopic.php?t=16973 27\. NES Colour: FAQ \- StickFreaks, https://stickfreaks.com/colour/nes-colour-palette-comparisons/faq 28\. NTSC video \- NESdev Wiki, https://www.nesdev.org/wiki/NTSC\_video 29\. Emulating NTSC video \- nesdev.org, https://forums.nesdev.org/viewtopic.php?t=14979 30\. DMA \- NESdev Wiki, https://www.nesdev.org/wiki/DMA