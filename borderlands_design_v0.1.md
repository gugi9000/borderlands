# Grænseland — spildesign v0.2

**Status:** v0.2 implementeret som konsolsimulation; balanceværdier er foreløbige  
**Genre:** Asynkront, abstrakt multiplayer-territoriespil  
**Første mål:** Simulér fire bots med de tre beskrevne grundstrategier, og undersøg balance, forsyning og kamp.

## 1. Spillets idé

Spillerne udvider territorier fra ukrænkelige hjemmebaser på et fælles bræt. Vægge begrænser adgang og skaber flaskehalse. Ressourcer produceres af territorium og bruges på ekspansion, angreb, forsvar og outposts. Et områdes mulighed for at ekspandere afhænger af forbindelse til en forsyningskilde.

Spillet er designet til, at spillere kan forsvinde i 20 minutter eller længere uden at stoppe spillet. Hver spiller har en **stående auto-play-strategi**, som også gælder, når browserfanen lukkes. Manuelle handlinger kan overstyre botten, men følger de samme regler og priser.

Spillerne aftaler **spillets varighed ved start**. Vinderen har flest forsynede felter ved sluttidspunktet; hvert forsynet felt tæller ét point. Uforsynede felter beholdes, men giver ingen sejrspoint. Ressourcer er et middel til at vinde, ikke sejrspoint.

## 2. Bræt og bevægelse

- Første prototype: kvadratisk gitter med fire ortogonale naboer pr. felt; eksempelvis 11 × 11 eller større.
- Fire ukrænkelige hjemmebaser (A–D). Ingen kan overtage en andens hjemmebase.
- Vægge fylder et helt felt (`#`) og forhindrer bevægelse, forsyning og erobring gennem feltet.
- Layouts kan variere. Alle baser skal kunne nå det øvrige bræt via lovlige ruter.
- Hexagonale felter er en mulig senere variant. Hold nabo-/kantlogik adskilt fra spilreglerne.
- Territorium udvides fra allerede ejede, **forsynede** felter. Ingen frie placeringer hvor som helst på brættet.

## 3. Territorium og forsyningsnet

Et felt kan være neutralt, ejet af én spiller eller blokeret af en væg. Et sammenhængende område er forbundet gennem egne felter uden vægfliser imellem.

**Omringning:** Når et helt sammenhængende neutralt område er afskåret fra kortets kant, og alle felter langs områdets grænse enten tilhører den samme spiller eller er vægge, erobrer spilleren automatisk hele området. Et område med en neutral vej til kanten eller en grænse, der indeholder en anden spillers felt, erobres ikke. Omringning påvirker aldrig allerede ejede felter; fjendtlige felter skal stadig erobres med et angreb.

**Forsyningskilder:**

1. **Hjemmebase:** Ukrænkelig og permanent forsyningskilde.
2. **Outpost:** Erobrelig, selvstændig forsyningskilde, der fortsætter med at fungere, selv når forbindelsen til hjemmebasen brydes.

Et felt er **forsynet**, når det er forbundet til mindst én aktiv forsyningskilde gennem egne felter. En afskåret enklave uden forsyningskilde beholder sit ejerskab og passive forsvar, men kan ikke producere, udvide eller bygge/forstærke.

Et ejet felt bliver neutralt efter **5 afsluttede ticks i træk uden forsyning**. Genoprettet forsyning nulstiller feltets tæller. Bots prioriterer at genåbne forbindelsen, før de udfører almindelige stance-handlinger. Neutralisering fjerner også befæstning og eventuel outpostfunktion.

### Lokal økonomi

- Ressourcer produceres over tid af forsynet territorium.
- Produktion pr. felt stiger med afstanden fra spillerens **oprindelige hjemmebase**, ikke fra nærmeste outpost.
- Afstand måles som korteste lovlige rute gennem brættets kanter (vægge tæller som blokering), uafhængigt af nuværende ejerskab.
- Ressourcer tilhører det sammenhængende forsyningsnet, hvor de produceres. Et afskåret outpost-net kan ikke bruge eller overføre sit lager til andre net.
- Når netværk forbindes, kan deres lagre sammenlægges. Fordeling af lager ved opsplitning er endnu ikke fastlagt (se åbne spørgsmål).

**Foreløbig produktionsformel:** `produktion(felt) = 1 + floor(afstand_fra_hjemmebase / 4)` pr. produktionsrunde. Afstand og intervaller skal balanceres; basens egen produktion behandles særskilt.

## 4. Outposts

- Bygges på et eget, forsynet felt.
- Kan producere og understøtte ekspansion uafhængigt af hjemmebasen.
- Kan angribes og ødelægges; kun hjemmebaser er ukrænkelige.
- En isoleret outpost er afhængig af sit **opland** (forbundet eget territorium). Hvis oplandet er under en minimumsstørrelse i for mange på hinanden følgende runder, forgår outposten.
- Genoprettet forbindelse til hjemmebasen eller udvidet opland kan redde den.
- Når en outpost forgår, beholder spilleren almindelige felter, men området kan miste sin forsyningsstatus.

**Foreløbige testværdier:** Mindst 5 felter inklusive outpostfeltet; forgår efter 3 produktionsrunder under minimum, **kun når den ikke er forbundet til hjemmebasen**. Bygning koster 15 forsyning og 1 handlingspoint. Højst 2 outposts pr. spiller. Krav om mindst 3 egne nabofelter ved bygning er et muligt, endnu ikke fastlagt balancegreb.

## 5. Handlinger og kamp

Alle handlinger forbruger et **handlingspoint (AP)**. Spillere får +1 AP ved starten af hvert tick, uanset om de er online, og bots kan derefter købe ekstra AP for forsyning, op til 6 AP i alt. Prisen stiger for hvert AP-køb i samme tick og nulstilles ved næste tick. Hvert køb betales fuldt ud fra ét forsyningsnetværk; lagre fra adskilte netværk kan ikke lægges sammen til et køb. Boten køber så mange AP, som den har AP-kapacitet og råd til. Automatiske og manuelle handlinger bruger samme AP og forsyning.

**Foreløbige testværdier:** +1 AP pr. tick; maksimum 6 AP i alt.

| Ekstra AP i samme tick | Pris | Samlet pris |
|---:|---:|---:|
| 1. | 20 | 20 |
| 2. | 40 | 60 |
| 3. | 80 | 140 |
| 4. | 160 | 300 |
| 5. | 320 | 620 |

| Handling | Forsyning | AP | Foreløbig regel |
|---|---:|---:|---|
| Indtag neutralt felt | 2 | 1 | Kræver tilstødende forsynet eget felt og ingen blokerende væg |
| Angrib fjendtligt felt | 5 (basis) | 1 | Kræver tilstødende forsynet eget felt; ekstra forsyning kan købe angrebsstyrke |
| Befæst eget felt | 3 | 1 | Giver +1 forsvar; detaljer om loft/varighed er åbne |
| Byg outpost | 15 | 1 | Kræver forsynet eget felt og øvrige byggekrav |

### Første deterministiske kampmodel

- **Forsvarsstyrke:** `max(1, 1 + min(2, antal_egne_naboer) + befæstning + hjemmebasebonus - udmattelse)`, hvor hjemmebasebonus er +1 på baser og 0 ellers.
- `udmattelse = 1` for en isoleret enklave uden tilgængelig forsyningsreserve; ellers `0`.
- Vægge blokerer nabostøtte.
- **Angrebsstyrke:** Købes med forsyning. Første testforslag: 5 forsyning giver angrebsstyrke 2, hver ekstra 2 forsyning giver +1 styrke, maksimum 6. Angriberens felt skal være forsynet.
- **Angreb lykkes kun hvis angrebsstyrke > forsvarsstyrke.** Lighed vinder forsvareren.
- Ingen terninger i første version.
- En erobret outpost mister sin outpostfunktion; angriberen overtager feltet som almindeligt territorium.

**Vigtigt:** Reglen om forsyningsreserve og udmattelse er ikke endeligt fastlagt. Den enkleste v0.1-variant kan give isolerede områder deres normale passive forsvar uden reserve eller udmattelse. Test begge muligheder.

## 6. Auto-play-stances

En stance er valgt på forhånd og kører altid, medmindre spilleren bruger en manuel handling. Browserlukning og fravær kræver **ingen særlig kommando**.

| Stance | Primært mål | Typisk adfærd | Risiko |
|---|---|---|---|
| **Explorer** | Nå kortets yderkant via handels-/forsyningsruter | Udvid gennem neutrale felter; vælg ruter uden om modstandere | Lange, sårbare korridorer |
| **Aggro** | Erobre fjendtligt territorium | Angrib svage felter og forsyningsflaskehalse | Dyre angreb og svag egen ekspansion |
| **Base-builder** | Skabe kompakte, produktive områder | Udvid omkring hjemmebase/outposts; byg og beskyt outposts | Langsommere adgang til fjerne felter |

Mulige senere stances: Guardian, Saboteur, Merchant. De er **ikke** nødvendige for første simulation.

**Fælles botregler (forslag):** Undgå ulovlige handlinger; vælg kun handlinger med tilstrækkelig lokal forsyning og AP; prioriter at undgå nært forestående outpost-kollaps, hvis et lovligt redningstræk findes. Undgå at bots kan bruge ressourcer fra andre, afskårne netværk.

**MVP-testopstilling:** Fire bots bruger de tre implementerede stances; én stance er derfor repræsenteret af to bots i hver kamp. Den dobbelte stance og spillerplacering roterer gennem 12 opstillinger, så hver stance og startposition kan sammenlignes.

## 7. Tid, score og balance

- Varighed vælges ved spilstart, fx 1, 2, 4 eller 8 timer. I konsolsimulationen bruges et fast antal ticks svarende til den valgte varighed.
- Alle forsynede felter giver **1 sejrspoint**, uanset afstand. Hjemmebaser tæller også som ét felt. Uforsynede felter giver ingen point; felter forsynet af et selvstændigt outpost-net tæller med.
- Fjerne felter giver **flere ressourcer, ikke flere sejrspoint**.
- Ingen spiller kan elimineres, fordi hjemmebasen er ukrænkelig.
- Aktiv spiller får bedre mulighed for taktiske overstyringer, **ikke** ekstra AP eller billigere handlinger.
- Første botforsøg bør måle, om én stance konsekvent dominerer, og om førende spillere accelererer urimeligt hurtigt.

## 8. Konsolsimulation: første milepæl

**MVP:** Fire bots, et kvadratisk kort indlæst fra tekstfil, en deterministisk spilsløjfe og en interaktiv Bevy-GUI. GUI'en viser bræt, spillerstatus, feltdetaljer og hændelseslog; batch-tilstand gør stance-matchups målbare.

1. Indlæs kort fra en tekstfil med felter, vægge og fire hjemmebaser.
2. Hold styr på ejerskab, forsyningsnet, lokale lagre, AP og outposts.
3. Beregn forsyning med flood-fill efter ejerskifte eller ændring af outpoststatus.
4. Udfør produktion og AP-regenerering i faste ticks.
5. Lad hver bot vælge lovlige handlinger efter sin stance.
6. Afvikl kamp deterministisk og opdatér kortet.
7. Kontrollér outpostens opland og nedtælling til kollaps.
8. Stop ved aftalt varighed; vis point og nøgletal.

**Tick-rækkefølge:** (a) +1 AP (maksimum 6) og produktion, (b) hver bot planlægger alle sine ordrer fra samme tick-starttilstand og køber på forhånd kun det AP, der er nødvendigt til de planlagte ordrer, (c) alle ordrer og angrebsbud betales ved indsendelse og afvikles samlet, (d) neutrale områder omringet efter afviklingen erobres, forsyningsnet opdateres efter ejerskifter/outpostændringer, (e) outpost-overlevelse, (f) log og slutkontrol. Nye felter, der erobres i et tick, kan først bruges som udgangspunkt for ordrer i næste tick. Hver spiller må kun anvende AP på samme felt én gang pr. tick og må ikke genbruge et felt, spilleren brugte i det foregående tick, uanset handlingstype. Alle indsendte ordrer bruger AP og forsyning, også hvis et angreb taber, eller flere spillere får samme bud. Ved kamp vinder det højeste unikke angrebsbud, hvis det overstiger forsvaret; ved lighed vinder forsvaret, og et neutralt felt forbliver neutralt. De indsendte angrebsstyrker offentliggøres først efter kampafgørelsen.

**Log/metrics:**

- Kontrollerede felter og score over tid pr. spiller.
- Ressourceproduktion, lagre, forbrug og uudnyttede AP.
- Neutrale erobringer, angreb, succesrate og tabte felter.
- Afskårne felter og tid uden forsyning.
- Outposts bygget, isoleret, reddet og mistet.
- Antal ticks før hver stance når yderkanten.
- Sejrsandel på tværs af mange seeds og varierede startpositioner.

**Kommandoer og GUI:** `cargo run -- --ticks 200` starter Bevy-GUI'en i fullscreen på standardbanen `maps/crossroads.map`. Kortet fylder cirka 68 % af vinduet; spillerstatus og detaljer om det valgte felt vises i panelet helt til højre. De seneste 20 aktivitetshændelser vises oven på kortet øverst til venstre. Klik på et felt for at se ejer, forsyningsværdi, forsyning, forsvar, base, outpost og befæstning. Scroll på kortet for at zoome, og klik/træk for at panorere. `F12` skifter mellem fullscreen og vindue; `F3` gør aktivitetsloggens baggrund mere gennemsigtig (helt transparent efter gentagne tryk), og `F4` gør den mindre gennemsigtig igen. `Space` udfører næste tick, `R` pauser/fortsætter, `N` starter næste roterede opstilling, og `Q` afslutter. `--delay-ms 120` justerer autoplay-tempoet. Vælg en tekstbane med `--map maps/dit-kort.map`; samme kortvalg virker i batchtilstand, fx `cargo run -- --batch 120 --ticks 200 --map maps/crossroads.map`. Kortfilen definerer altid størrelse, vægge og baseplacering. `--generated-size 17` er en separat mulighed til et genereret 17 × 17-testbræt med hjørnebaser; den kan ikke kombineres med `--map`. Det tidligere `--board-size` afvises eksplicit for at undgå at vælge et genereret kort ved en fejl. Standardkampen varer 200 ticks. Hver kørsel skriver til en unik undermappe i `logs/`, med to filer pr. spil: `game-NNNN-activity.log` og `game-NNNN-choices.log`.

**Grafikaktiver:** Første GUI-tegnebræt kræver ingen eksterne billeder: Bevy tegner felter som farvede rektangler. Alle tekster og basemarkører bruger den medleverede Noto Sans JP-skrifttype, som understøtter æ, ø, å (og store bogstaver) samt hiragana. Skrifttypen er indlejret i programmet, så den ikke afhænger af installerede systemfonte eller arbejdsmappe. Den kommer fra [Google Fonts](https://github.com/google/fonts/tree/main/ofl/notosansjp); font og SIL Open Font License ligger i `assets/fonts/`. Kortet bevarer referencebilledets mørke neutralsfelter, grå vægge og spillerfarver. Uforsynede ejede felter vises med 40 % af ejerfarvens RGB-lysstyrke, mens forsynede felter beholder den fulde farve. Hvis den visuelle stil senere skal poleres, kan vi tilføje en tilesheet eller individuelle PNG/SVG-teksturer til neutralt felt, væg, base, outpost og befæstning; fire spillerfarvede markørvarianter; samt valgfri lyd.

**Bygningsstatus:** Feltdetaljerne samler bygninger og befæstning i én statuslinje: `[Ubebygget]`, `[Base]`, `[Outpost]` eller `[Befæstning]`. Flere egenskaber vises sammen, fx `[Outpost | Befæstning]`; neutrale felter vises som `[Ubebygget]`. Vægfelter vises fortsat som vægge.

**Feltgrænser:** Det mørke mellemrum mellem felter er 1,5 kortenheder og skalerer med zoom.

**Feltmarkører:** Outposts vises med et lille lyst tårn øverst på feltet, og befæstninger med en takket mur nederst. Symbolerne har mørke kanter for kontrast og kan vises samtidig; de opdateres ved byggeri, erobring og outpost-kollaps. Basernes bogstaver bevares. Det valgte felt markeres med en gul ramme uden for feltets kant, ikke en farvet flade; ejerfarve og nedtoning af uforsynede felter bevares. Symboler og ramme følger kortets zoom og panorering.

**Banegenerator:** `cargo run --bin generate-map -- 35 "Grænseland"` gemmer `maps/grænseland-35.map`; indlæs den med `cargo run -- --map maps/grænseland-35.map`. Input er en ulige størrelse fra 11 til 101 og et navn. Navnet trimmes og konverteres til lower case; bogstaver (inklusive Unicode), tal, mellemrum, bindestreger og underscores accepteres. Eksisterende filer overskrives ikke. Det genbrugelige Rust-modul er `borderlands::map_generator`; `generate(size, name)` returnerer navn, størrelse, seed og færdig korttekst.

Generatoren bruger 64-bit FNV-1a på det lower-case navns UTF-8-bytes og fortsætter hashberegningen med størrelsen som otte little-endian bytes. Dette seed driver en deterministisk SplitMix64-sekvens. Grundkvadratet består af øverste venstre kvartal plus den ekstra kolonne til højre og række nederst, som bliver kortets fælles midterakser. A placeres tilfældigt uden for midterakserne og spejles til B (højre), C (nederst) og D (begge akser). Vægge spejles på samme måde. Cirka 20 % af felterne bliver vægge; en væg accepteres kun, hvis kvartalets åbne felter stadig er forbundet. Det fælles midterfelt holdes åbent, så alle åbne felter og alle fire baser i den samlede bane er forbundet. Samme navn og størrelse giver samme kort, uanset navnets store/små bogstaver. Seedet gemmes i kortets kommentarlinje. Den eksisterende `--generated-size`-testbane er uændret.

**Feltantal og sejr:** For hver spiller vises forsynede felter, uforsynede felter og felter i alt. Forsynede felter afgør sejren; uforsynede felter bruges ikke til at bryde en ligestilling. Slutloggen angiver forsynet score samt uforsynede og totale felter. Batchresultater viser gennemsnit for alle tre feltantal og afgør vindere efter forsynede felter. Choices-loggens `net` er fortsat ændringen i alle ejede felter, ikke ændringen i sejrspoint.

**Botstrategier og forsynet score:** Bots handler kun fra forsynede områder. Alle stances prioriterer nu genforbindelse af egne uforsynede felter via neutrale erobringer eller angreb. De vælger først korteste rute målt i naboskridt fra kandidatfeltet til en egen uforsynet enklave, dernæst laveste handlingspris og den deterministiske felt-tie-breaker. Ruter krydser ikke vægge, fjendtlige hjemmebaser eller allerede forsynede egne områder. Almindelige stance-prioriteter bruges, når ingen finansierbar genforbindelseshandling findes; reglerne for samtidige ordrer og AP er uændrede. Ældre balancetal nedenfor blev målt med alle ejede felter som score og er ikke direkte sammenlignelige med den nye sejrsvurdering.

**Forsyningsnedtælling:** Efter outpost-overlevelse vurderes alle felters forsyning én gang pr. tick. Et uforsynet felt viser antal ticks tilbage (5 til 1) med dæmpet grå tekst på kortet og i feltdetaljerne. Ved det femte afsluttede tick bliver feltet neutralt, og hændelsen logges; genetablering senest i dette ticks ordreafvikling redder feltet. Forsyning fra en aktiv outpost tæller som normal forsyning; hvis outposten kollapser, begynder nedtællingen i kollapsets tick. Neutralisering sker efter omringning, så felterne bliver ikke straks genindtaget ved omringning i samme tick. Ved ejerskifte starter en eventuel ny uforsynet periode forfra.

**Statistik og logfiler:** GUI'en viser felter vundet i kamp (erobrede fjendefelter) og felter overtaget ved opdagelse (neutrale direkte erobringer og omringningserobringer) som separate statistikker uden bonus samt de seneste 20 hændelser i et gennemsigtigt justerbart overlay øverst til venstre på kortet. Klik på et felt for at se dets detaljer i sidepanelet. Activity-filen indeholder hele hændelsesloggen for spillet. Choices-filen indeholder én linje pr. spiller pr. tick med AP, antal tilgængelige neutrale/fjendtlige/outpost-/befæstningsmål, alle valgte ordrer samt `net`, ændringen i spillerens kontrollerede felter det tick. Logning påvirker ikke spillets udfald.

**Kortformat:** Rektangulær eller kvadratisk UTF-8-tekstfil; hver række er én brætrække. `.` er frit felt, `#` er en fuld vægflise, og `A`, `B`, `C`, `D` placerer de fire baser. Bots starter på de markerede basefelter. Whitespace i rækker ignoreres; tomme linjer samt linjer der starter med `;` eller `//` ignoreres. Bredde og højde udledes af filen og skal være ulige mellem 11 og 101. `maps/crossroads.map` er 35 × 31 felter med baserne samlet omkring den centrale korsformede væg.

**Testcases:** Afskåret korridor; isoleret enklave uden outpost; selvstændig outpost med tilstrækkeligt opland; outpost der sulter og forgår; genetableret forbindelse; to forsyningsnet der forbindes; væg der blokerer nabo/angreb; lige kampstyrke; alle bots med samme stance; roterede startpositioner.

## 9. Åbne spørgsmål

1. Hvordan fordeles opsparede ressourcer, når et forsyningsnet splittes? Muligheder: lager fysisk placeret ved forsyningskilder, proportional fordeling eller kun lager ved baser/outposts.
2. Skal isolerede felter have en forsvarsreserve, eller blot bevare passivt forsvar uden mulighed for forstærkning?
3. Hvordan beregnes præcis angrebsstyrke, og hvor meget må man købe pr. angreb?
4. Skal befæstning have maksimum, forbruges ved angreb eller være permanent?
5. Hvilke outpost-byggekrav og hvilket oplandsminimum giver god balance?
6. Skal automatiske bots bruge alle AP straks eller holde en lille reserve til manuelle træk?
7. Hvordan håndteres samtidige handlinger retfærdigt i fremtidig multiplayer? (Serveren bør være autoritativ.)
8. Hvilken brætstørrelse og hvilke væglayouts fungerer bedst med 3–8 spillere?
9. Hvad sker der ved uafgjort score?
10. Hvilken grad af tilfældighed, hvis nogen, skal indføres efter den deterministiske prototype?

## 10. Uden for v0.2

Browser-UI, konti, netværksmultiplayer, animationer, avancerede stances, hex-brætter, matchmaking og kosmetik. Arkitekturen må gerne understøtte disse senere, men første opgave er at finde ud af, **om spillet er sjovt og nogenlunde balanceret**.

## 11. MVP-beslutninger

Følgende valg lukker åbne spørgsmål for at gøre simulationen deterministisk og testbar; de er balanceantagelser, ikke endelige spildesignbeslutninger.

- Standardbanen er `maps/crossroads.map`, et 35 × 31-kort. Kortfilen definerer størrelse, fulde vægfelter og hjemmebasernes placering; alle baser, afstande og forsyningskilder udledes af A–D-markørerne i filen. `--map FILE` indlæser andre rektangulære eller kvadratiske baner. `--generated-size N` genererer i stedet et ulige kvadratisk bræt fra 11 × 11 til 101 × 101. Bevy-kortet kan zoomes og panoreres, så store baner kan ses i GUI'en.
- Ét tick er én handlingsfase. Alle starter med 6 AP og 12 forsyning; ved tickets start får hver spiller +1 AP (maksimum 6), derefter producerer territoriet forsyning. Hver bot planlægger alle sine ordrer fra samme tick-starttilstand og køber på forhånd kun AP til de ordrer, den kan finansiere. Priserne pr. ekstra AP i ticket er 20, 40, 80, 160 og 320; prisrækken starter forfra hvert tick. Hvert AP-køb skal betales fra ét forsyningsnetværk; netværkslagre sammenlægges ikke. Botten forpligter sig til alle sine planlagte ordrer, og nye felter eller forsyningsforbindelser påvirker først næste tick. Hver spiller må bruge AP på et bestemt felt højst én gang pr. tick og må ikke genbruge et felt, der blev brugt i det foregående tick, uanset om handlingen er erobring, angreb, befæstning eller byggeri. Begrænsningen er pr. spiller (ikke global). Alle indsendte ordrer bruger AP og forsyning, også hvis et angreb taber eller en neutral erobring bliver uafgjort.
- Forsynet territorium producerer 1 forsyning pr. felt pr. tick, plus 1 for hver fire lovlige ruters afstand fra spillerens A–D-base i den indlæste kortfil. Hjemmebasen producerer 2. Afstands-BFS starter på samme markerede basefelt og går ikke gennem `#`-vægfelter; afstand følger ikke ejerskab. Forsyningsnetværk starter ligeledes på de markerede A–D-felter.
- Lageret tilhører det forsynede netværk. Når et net splittes, fordeles lageret proportionalt efter antallet af tidligere forsynede felter, der stadig er i hvert nyt net; ved sammenkobling lægges netlagrene sammen. Et lager uden tilbageværende forsynede felter går tabt.
- Isolerede felter beholder normal passiv kampstyrke, men producerer ikke og kan ikke handle. Efter fem ticks i træk uden forsyning bliver de neutrale. Genoprettet forsyning nulstiller tælleren. Der er ingen udmattelsesstraf eller forsyningsreserve i MVP'en.
- Et sammenhængende neutralt område erobres straks, når det ikke kan nå kortets kant gennem neutrale felter, og hele dets grænse består af vægge og felter ejet af én og samme spiller. Områder med blandet spillergrænse eller neutral udgang til kanten forbliver neutrale; allerede ejede (herunder fjendtlige) felter er aldrig omringningserobringsmål. Erobrede felter tæller med i spillerens erobringer og forsyningsnet.
- Neutral erobring koster 2 forsyning og 1 AP. Angreb starter ved styrke 2 for 5 forsyning; hver styrke over 2 koster yderligere 2, op til styrke 6. Angreb skal have højere styrke end forsvar; lighed vinder forsvareren.
- Forsvar er `max(1, 1 + min(2, egne naboer) + befæstning + hjemmebasebonus)`, hvor hjemmebasebonus er +1 på baser og 0 ellers. Befæstning koster 3 forsyning og 1 AP, giver permanent +1 og kan kun bygges én gang pr. felt.
- Outposts koster 15 forsyning og 1 AP, kræver et eget forsynet felt med mindst tre egne naboer, og der kan højst være to pr. spiller. Et isoleret outpost-net med færre end fem felter inklusive outposten kollapser efter tre ticks; hjemmeforbundne outposts kollapser ikke.
- Bots forsøger først stance-specifikke handlinger, når de er Base-builder (byg/forstærk) eller Aggro (angrib); ellers prioriterer de neutrale erobringer. Explorer vælger den nærmeste rute mod kanten og byder kun den styrke, der skal til for at slå forsvaret; Aggro vælger svage fjendefelter og byder én styrke over minimum, når lageret tillader det; Base-builder foretrækker kompakt territorium.
- Lige gode udforsknings- og angrebsfelter afgøres med en stabil 64-bit hash af simulationsnummeret (seed), tick, spiller-ID og feltets x/y-koordinater. Hashen bruges først efter alle strategiske kriterier: udforskningens stance-prioritet eller angrebets forsvarsstyrke og pris. Den ændrer ikke prioriteten mellem handlingstyper, AP-køb eller kampafvikling. Ved hashkollision bruges feltindekset som sidste tie-breaker. Samme simulation er reproducerbar; forskellige ticks, spillere og simulationsnumre kan vælge forskellige lige gode felter uden fast geografisk retning. Batchkampe roterer dublet-stance og spillerplacering gennem 12 opstillinger; ligestilling om højeste score krediteres alle vindende stances.
- Ældre baseline før samtidige ordrer (`--batch 12 --ticks 80`): Explorer vandt 7 kampe, Base-builder 5 og Aggro 0; gennemsnitligt territorium pr. bot var hhv. 36,69, 37,88 og 16,00 felter. Efter samtidige ordrer gav samme korte batch Aggro 12/12 sejre og gennemsnitligt 188 felter mod Explorers 8,06 og Base-builders 3,00. Det er et tydeligt balanceproblem, som kræver opfølgning; begge kørsler er små sanity-runs, ikke statistiske konklusioner.
- GUI-autoplay starter automatisk, når simulationen startes. Hver tick afvikles samlet efter `--delay-ms`; `Space` udfører næste tick manuelt, mens `R` pauser/fortsætter autoplay. GUI'en starter i fullscreen; `F12` skifter vinduestilstand, og `F3`/`F4` justerer aktivitetsloggens baggrundsgennemsigtighed.

---

*Designprincip: Man må altid lukke fanen. Spillet fortsætter, og ens valg har stadig konsekvenser.*
