(function(){
  "use strict";

  /* The device art lives in device.svg but has to end up INSIDE this document. Its
     gradients read CSS custom properties (.sl { stop-color: var(--shell-light) } and
     friends), and an external <use href="device.svg#lidArt"> renders into a shadow
     tree that never sees this page's stylesheet — every colourway would come out
     black. So: fetch it, inline it, then re-point the <use> elements at the defs that
     have just arrived, because they resolved to nothing while it was missing. */
  fetch("device.svg")
    .then(function(r){ return r.text(); })
    .then(function(txt){
      var holder = document.createElement("div");
      holder.innerHTML = txt;
      var defs = holder.querySelector("svg");
      if (!defs) return;
      document.body.insertBefore(defs, document.body.firstChild);
      var uses = document.querySelectorAll("use");
      for (var i = 0; i < uses.length; i++) {
        var h = uses[i].getAttribute("href");
        uses[i].removeAttribute("href");
        uses[i].setAttribute("href", h);
      }
    })
    .catch(function(){});
})();

(function(){
  "use strict";

  /* Each step names its clip; the files live in site/media/. Nothing here needs a
     manifest: if a clip is missing the video errors, stays hidden, and the drawn
     screen underneath carries on doing the job. That is why the page ships before a
     single frame has been recorded.

     Record at 720x480 — the panel's native size — and the fit is exact. */

  /* --------------------------------------------------------- colourway --- */

  /* Every RG SP colourway is metallic, so each one is a light tint, a base and a
     shade rather than a single hex. `etch` is the printed lettering and the speaker
     holes, which have to flip to white once the shell goes black. */
  var SHELLS = [
    { name:"silver",     light:"#e2e5e9", base:"#b8bcc2", shade:"#7e838b", btn:"#c2c6cc", btnHi:"#e9ecf0", etch:"rgba(0,0,0,.46)" },
    { name:"pink",       light:"#f6d3dd", base:"#e6a6b8", shade:"#a76d7e", btn:"#dbd1d5", btnHi:"#f2eaed", etch:"rgba(0,0,0,.44)" },
    { name:"light blue", light:"#dbe9f2", base:"#a8c4d8", shade:"#6d8ba1", btn:"#d0dae3", btnHi:"#edf3f8", etch:"rgba(0,0,0,.44)" },
    { name:"black",      light:"#5a5d64", base:"#34363b", shade:"#16171a", btn:"#3c3e43", btnHi:"#5c5f66", etch:"rgba(255,255,255,.42)" }
  ];

  /* Random per load, the way the device you actually own was one of four in a bin.
     `?shell=pink` pins it, which is how a particular colourway gets looked at on
     purpose. */
  var want = (location.search.match(/[?&]shell=([^&]+)/) || [])[1];
  var shell = SHELLS[Math.floor(Math.random() * SHELLS.length)];
  if (want) {
    want = decodeURIComponent(want).replace(/\+/g, " ").toLowerCase();
    for (var si = 0; si < SHELLS.length; si++) {
      if (SHELLS[si].name === want) { shell = SHELLS[si]; break; }
    }
  }

  var root = document.documentElement.style;
  root.setProperty("--shell-light", shell.light);
  root.setProperty("--shell-base",  shell.base);
  root.setProperty("--shell-shade", shell.shade);
  root.setProperty("--btn",         shell.btn);
  root.setProperty("--btn-hi",      shell.btnHi);
  root.setProperty("--etch",        shell.etch);

  /* `?pose=flat` swaps the hero device to the straight-on pose the guide uses, so a
     pose can be looked at without editing the page. */
  var pose = (location.search.match(/[?&]pose=(flat|hero)/) || [])[1];
  if (pose) {
    var heroDev = document.querySelector(".hero-device .device");
    if (heroDev) heroDev.className = "device pose-" + pose;
  }

  /* ------------------------------------------------------------- hero --- */

  var reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  /* One clip on a loop. Held back until the lid has finished opening: the screen is on
     the inside of it, so revealing any earlier just plays the thing into the back of a
     shut lid. 1.9s is the animation's own 0.4s delay plus its 1.5s swing.

     The element carries the clip's own first frame as its poster, so the reveal is on a
     timer rather than on loadeddata. A slow connection then shows a true frame of slot at
     the moment the lid opens instead of a black panel, and a clip that never arrives
     leaves that frame up rather than nothing. play() before the data lands is fine — the
     poster holds until there is a frame to paint over it. */
  var heroVid = document.querySelector(".hero-screen");
  if (heroVid) {
    if (reduced) {
      heroVid.style.opacity = "1";        /* the poster, and no motion */
    } else {
      setTimeout(function(){
        heroVid.style.opacity = "1";
        var p = heroVid.play();
        if (p && p.catch) p.catch(function(){});
      }, 1900);
      heroVid.src = "media/main.mp4";
      heroVid.load();
    }
  }

  /* ------------------------------------------------------------ guide --- */

  var vid   = document.querySelector(".guide-screen");
  var gstage = document.querySelector(".guide-stage");
  var gdev  = document.querySelector(".guide-stage .device");
  var steps = Array.prototype.slice.call(document.querySelectorAll(".step"));
  if (!vid || !steps.length) return;

  var current = null;

  /* The lid's transition in the stylesheet. Kept in step by hand: a swing that outlasts this
     shows the gap again at its tail. */
  var LID_SWING_MS = 1150;
  var swingTimer = null;

  vid.loop = true;

  /* Starting playback has to be something that can be asked for repeatedly and from any
     direction, because a single attempt has too many ways to come to nothing: a snap that
     crosses several steps calls load() again and aborts the play that was pending, and
     while the hero's device is still travelling the guide's is display:none, so there is no
     box to start playing in. Idempotent — a clip already running is left alone. */
  var playRetries = [];
  function ensurePlaying(){
    if (reduced || !vid.getAttribute("src") || !vid.paused) return;
    var p = vid.play();
    if (p && p.catch) p.catch(function(){});
  }
  /* Asked again on every signal that the element might now be able to start. */
  vid.addEventListener("loadeddata", ensurePlaying);
  vid.addEventListener("canplay", ensurePlaying);
  vid.addEventListener("canplaythrough", ensurePlaying);
  function nudgePlayback(){
    for (var i = 0; i < playRetries.length; i++) clearTimeout(playRetries[i]);
    playRetries = [];
    ensurePlaying();
    /* And again shortly after, for the cases no event covers: a play aborted by the next
       load, or an element that had no box when it was first asked. */
    playRetries.push(setTimeout(ensurePlaying, 220), setTimeout(ensurePlaying, 800));
  }
  /* Not hidden on error: the poster is a separate file and a real frame of the step, so a
     clip that is missing or unplayable still leaves the right picture on the screen. */

  /* Which layout is live. Both are wired up at once rather than torn down and rebuilt on
     resize: the stacked panels are display:none when side by side, and an element with no
     box never intersects, so their clips are never fetched. Crossing the breakpoint just
     starts and stops satisfying the observers. */
  var stacked = window.matchMedia("(max-width: 880px)");

  /* Each stacked card owns its clip. Fetched a little before it arrives, played only while
     it is actually on screen — six clips is 4MB, and one of them is 2MB on its own. Under
     reduced motion nothing is fetched at all and the poster stays, which is a real frame
     of the step either way. */
  var cards = Array.prototype.slice.call(document.querySelectorAll(".step-panel video"));
  if (cards.length && window.IntersectionObserver && !reduced) {
    /* Two observers, because rootMargin skews intersectionRatio: with the root expanded, a
       card sitting entirely off screen still reports a ratio of 1. Fetching wants the
       margin so the clip is there before you arrive; playing wants the truth. */
    var load = new IntersectionObserver(function(entries){
      for (var i = 0; i < entries.length; i++) {
        var v = entries[i].target, step;
        if (!entries[i].isIntersecting || v.getAttribute("src")) continue;
        step = v.parentNode.parentNode;
        var clip = step && step.getAttribute("data-clip");
        if (clip) { v.src = "media/" + clip; v.load(); }
      }
    }, { rootMargin: "400px 0px" });
    var run = new IntersectionObserver(function(entries){
      for (var i = 0; i < entries.length; i++) {
        var v = entries[i].target;
        if (entries[i].intersectionRatio >= .5) {
          var pr = v.play();
          if (pr && pr.catch) pr.catch(function(){});
        } else if (!v.paused) {
          v.pause();
        }
      }
    }, { threshold: [0, .5] });
    for (var ci = 0; ci < cards.length; ci++) { load.observe(cards[ci]); run.observe(cards[ci]); }
  }

  function show(step){
    /* Stacked, there is no pinned screen to drive and no dimming: every card stands on its
       own. The dots still track, which is all pick() is for down there. */
    if (stacked.matches) return;
    if (step === current) return;
    current = step;
    for (var i = 0; i < steps.length; i++) {
      steps[i].classList.toggle("is-active", steps[i] === step);
    }
    /* One step asks for the lid shut rather than a clip. Marked in the markup so
       renaming the step's heading cannot quietly break it. */
    if (gdev) {
      var wantShut = step.getAttribute("data-lid") === "shut";
      /* Opening. For the first half of the swing the lid's front is turned away and culled,
         so the back is the only face there is to see — but the back is only drawn while
         shut, to keep it off the screen when the lid is open and at rest. Without this the
         lid vanishes from the moment it starts opening until it comes past vertical. Hold
         the back for the length of the swing, then drop it again. Nothing to hold under
         reduced motion: there is no transition, so there is no gap to cover. */
      if (!wantShut && gdev.classList.contains("is-shut") && !reduced) {
        gdev.classList.add("is-swinging");
        clearTimeout(swingTimer);
        swingTimer = setTimeout(function(){ gdev.classList.remove("is-swinging"); }, LID_SWING_MS);
      }
      if (wantShut && dir < 0) {
        /* Arriving from below, you are coming back to a device you left closed, so it
           should already BE shut rather than replay the swing. Killing the transition
           for one frame is what makes it arrive shut instead of shutting; the reflow
           read is there to stop the two class changes being batched into one, which
           would animate anyway. */
        gdev.classList.add("no-swing");
        gdev.classList.add("is-shut");
        void gdev.offsetWidth;
        gdev.classList.remove("no-swing");
      } else {
        gdev.classList.toggle("is-shut", wantShut);
      }
    }

    var clip = step.getAttribute("data-clip");
    if (!clip) { vid.style.opacity = "0"; vid.removeAttribute("src"); return; }
    /* Each clip ships a still of its own first frame beside it. It is the poster, so it
       is what the screen shows until the clip has enough data to paint — and what it
       keeps showing if the clip never arrives. */
    var still = "media/" + clip.replace(/\.mp4$/, ".webp");
    if (reduced) {
      /* The still alone. Reduced motion loses the movement, not the picture. */
      if (vid.poster !== still) { vid.removeAttribute("src"); vid.poster = still; }
      vid.style.opacity = "1";
      return;
    }
    /* Consecutive steps can name the same clip — the lid step closes on whatever the
       step before it was already playing. Re-setting an identical src would reload and
       restart it, so the picture would jump at the very moment the lid starts to swing.
       Leaving it alone keeps the game running underneath the close. */
    var path = "media/" + clip;
    if (vid.getAttribute("src") === path) {
      /* Same clip as the step before — the lid step closes on whatever was already running,
         and re-setting the src would reload and restart it, so the picture would jump at the
         moment the lid starts to swing. Leaving the src alone is right; leaving the rest of
         the element alone is not. Leaving the guide hides the screen, and coming back to the
         very same clip takes this path — so the opacity has to be put back, or the clip
         plays on perfectly behind a screen that is still turned off. And playback needs
         asking for again, since arriving from the other direction it may be sitting paused
         with nothing left to fire an event. */
      vid.style.opacity = "1";
      nudgePlayback();
      return;
    }
    vid.poster = still;
    vid.src = path;
    vid.style.opacity = "1";
    vid.load();
    nudgePlayback();
  }

  /* Whichever section's centre is nearest the middle of the viewport is the one you
     are on: it drives the clip, the step highlight and the dots from one measurement.
     This began as an IntersectionObserver with a thin centre band, which is the tidier
     idiom, but its callbacks are not dependable after a large instant scroll — the
     step you jumped to could stay unlit. Measuring is deterministic: same answer at
     any scroll position, arrived at by any route, in any browser. rAF-throttled, so
     it costs one rect read per frame while scrolling and nothing at rest. */
  /* One selector, document order, so adding a section means adding markup and a dot
     and nothing else. Concatenating hand-picked lists meant every new section was two
     places to forget. */
  var sections = Array.prototype.slice.call(
    document.querySelectorAll(".hero, .step, .reference, .faq, .endnote"));
  var dots = Array.prototype.slice.call(document.querySelectorAll(".dots a"));
  var ticking = false;
  var lastY = window.pageYOffset || 0;
  var dir = 1;                       /* 1 = moving down the page, -1 = moving up */

  function clearSteps(){
    if (stacked.matches) return;
    for (var i = 0; i < steps.length; i++) steps[i].classList.remove("is-active");
    current = null;
    vid.style.opacity = "0";
    /* The lid stays as the last step left it for as long as the device is still on
       screen. Reaching the questions hands the page over to them while the shut device
       is often still pinned above, and springing it open there reads as the page
       glitching rather than as the guide ending. Reset only once the stage has gone —
       coming back down then replays the close from the top, which is the point of it. */
    if (gstage) {
      var sr = gstage.getBoundingClientRect();
      if (sr.bottom > 0 && sr.top < window.innerHeight) return;
    }
    if (gdev) {
      if (gdev.classList.contains("is-shut") && !reduced) {
        gdev.classList.add("is-swinging");
        clearTimeout(swingTimer);
        swingTimer = setTimeout(function(){ gdev.classList.remove("is-swinging"); }, LID_SWING_MS);
      }
      gdev.classList.remove("is-shut");
    }
  }

  /* A step's own box is not where its words are. On a phone the stage is pinned across
     the top of the screen and each step is its copy plus the gap to the next one, so the
     box centre sits in that gap, well below the text — measuring it lit the step whose
     copy had already gone behind the device. Measure first child to last child instead,
     which is the copy itself in both layouts: on a wide screen the step is flex-centred,
     so the two agree and nothing changes. */
  function extentOf(el){
    /* Stacked, a step is a whole card — panel and words together — and the dot should
       follow the card. Side by side the step box is 70vh of mostly air with the copy
       centred in it, so what matters is where the copy is. Measured from the heading
       rather than from firstElementChild, which up there is the card's own panel: hidden,
       and a hidden element measures as a zero rect at the top of the document. */
    var h2 = !stacked.matches && el.classList.contains("step") && el.querySelector("h2");
    if (!h2) {
      var r = el.getBoundingClientRect();
      return [r.top, r.bottom];
    }
    return [h2.getBoundingClientRect().top,
            el.lastElementChild.getBoundingClientRect().bottom];
  }

  function pick(){
    ticking = false;
    travel();
    var y = window.pageYOffset || 0;
    if (y !== lastY) { dir = y > lastY ? 1 : -1; lastY = y; }
    var h = window.innerHeight, best = -1, bestDist = Infinity;
    for (var i = 0; i < sections.length; i++) {
      var e = extentOf(sections[i]);
      if (e[1] < 0 || e[0] > h) continue;
      var d = Math.abs((e[0] + e[1]) / 2 - h / 2);
      if (d < bestDist) { bestDist = d; best = i; }
    }
    if (best < 0) return;
    for (var k = 0; k < dots.length; k++) dots[k].classList.toggle("is-on", k === best);
    var sec = sections[best];
    if (sec.classList.contains("step")) show(sec); else clearSteps();
  }

  /* ------------------------------------------------------- the handoff --- */

  /* Side by side, the hero's device and the guide's are meant to read as one object. The
     hero's leaves the flow, follows the scroll down and turns flat as it goes; the guide's
     takes over the instant it lands, in the same place at the same size, so the swap has
     nothing to show. Two elements, one device.

     Geometry is measured rather than derived — where the hero's device sits, and where the
     guide's sits once its stage is pinned — so it stays right whatever the layout does at
     a given width. Stacked there is no handoff: the guide has no pinned device to hand to.
     Reduced motion has none either; the two simply stay where they are. */
  var heroWrap = document.querySelector(".hero-device");
  var heroDev  = heroWrap && heroWrap.querySelector(".device");
  var heroLid  = heroDev && heroDev.querySelector(".dv-lid");
  var geo = null;

  function measure(){
    if (!heroWrap || !gdev || !gstage) return null;
    /* The guide's device is display:none while it waits, and a collapsed box measures zero.
       Stand it up for the length of the read. */
    var waiting = gdev.classList.contains("is-waiting");
    if (waiting) gdev.classList.remove("is-waiting");
    var hw = heroWrap.getBoundingClientRect(), st = gstage.getBoundingClientRect();
    var gW = gdev.offsetWidth, gH = gdev.offsetHeight, stW = gstage.offsetWidth;
    if (waiting) gdev.classList.add("is-waiting");
    return {
      /* document coordinates, so they survive any scroll position */
      fromLeft: hw.left,
      fromTop:  hw.top + (window.pageYOffset || 0),
      fromW:    heroWrap.offsetWidth,
      /* where the guide's device comes to rest: centred in a stage pinned at the top */
      toLeft:   st.left + (stW - gW) / 2,
      toW:      gW,
      toTop:    (window.innerHeight - gH) / 2,
      /* the run: the hero's own height, which is exactly where the guide begins */
      run:      document.querySelector(".hero").offsetHeight
    };
  }

  function rest(){
    if (heroDev) heroDev.style.cssText = "";
    if (heroLid) heroLid.style.transform = "";
    if (heroWrap) heroWrap.style.removeProperty("--glow");
    if (heroDev) heroDev.classList.remove("no-lid-anim");
    if (gdev) gdev.classList.remove("is-waiting");
  }

  function travel(){
    if (!heroDev || !gdev || stacked.matches || reduced) { rest(); return; }
    if (!geo) geo = measure();
    if (!geo || !geo.run) return;

    var y = window.pageYOffset || 0;
    var t = y / geo.run;
    t = t < 0 ? 0 : t > 1 ? 1 : t;

    if (t >= 1) {                       /* arrived: the guide's device has it from here */
      /* display, not visibility, for the same reason the guide's device uses it: the flake
         texture is an SVG filter and paints straight through a hidden ancestor. */
      heroDev.style.display = "none";
      if (gdev.classList.contains("is-waiting")) {
        gdev.classList.remove("is-waiting");
        /* The guide's device has only just been given a box. Anything asked of the clip
           while it had none needs asking again. */
        nudgePlayback();
      }
      return;
    }
    gdev.classList.add("is-waiting");
    heroDev.style.display = "";
    if (t > 0) heroDev.classList.add("no-lid-anim");

    /* Eased so it settles into the guide rather than arriving at full tilt. */
    var k = t * t * (3 - 2 * t);
    var lerp = function(a, b){ return a + (b - a) * k; };
    var dw = lerp(geo.fromW, geo.toW);

    heroDev.style.position = "fixed";
    heroDev.style.zIndex = "20";
    heroDev.style.left = lerp(geo.fromLeft, geo.toLeft) + "px";
    /* Its resting place in the hero, not its scrolled-away one. Blending from a position
       that is itself scrolling upward sent the device off the top of the screen and then
       pulled it back down — a dip, not a swoop. Held against the viewport it simply glides
       from where it stood to where it is going, while the page moves underneath. */
    heroDev.style.top  = lerp(geo.fromTop, geo.toTop) + "px";
    heroDev.style.width = dw + "px";
    heroDev.style.setProperty("--dw", dw + "px");
    heroWrap.style.setProperty("--glow", String(1 - k));
    heroDev.style.transform =
      "perspective(" + (dw * 2.5) + "px)" +
      " rotateZ(" + (13 * (1 - k)) + "deg)" +
      " rotateX(" + (6 * (1 - k)) + "deg)" +
      " rotateY(" + (-27 * (1 - k)) + "deg)";
    if (heroLid) heroLid.style.transform = "rotateX(" + (-22 * (1 - k)) + "deg)";
  }

  function onScroll(){
    if (!ticking) { ticking = true; requestAnimationFrame(pick); }
  }

  window.addEventListener("scroll", onScroll, { passive: true });
  window.addEventListener("resize", function(){ geo = null; onScroll(); }, { passive: true });
  /* The measurement needs the artwork's real height, which arrives with device.svg. */
  window.addEventListener("load", function(){ geo = null; onScroll(); });
  pick();
})();
