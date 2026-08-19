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

  /* One clip on a loop behind the drawn carousel. Held back until the lid has finished
     opening: the screen is on the inside of it, so revealing any earlier just plays the
     thing into the back of a shut lid. 1.9s is the animation's own 0.4s delay plus its
     1.5s swing. */
  var heroVid   = document.querySelector(".hero-screen");
  var heroDrawn = document.querySelector(".hero-device .device-screen-content");
  if (heroVid && !reduced) {
    /* The drawn carousel is a stand-in for a clip that has not been recorded. Once we
       commit to loading one it should never be seen, so it goes now rather than showing
       a mock-up for a beat and then being replaced by the real thing. It comes back
       only if the clip turns out to be missing or unplayable. */
    if (heroDrawn) heroDrawn.classList.add("fade-out");
    heroVid.addEventListener("loadeddata", function(){
      setTimeout(function(){
        heroVid.style.opacity = "1";
        var p = heroVid.play();
        if (p && p.catch) p.catch(function(){});
      }, 1900);
    }, { once: true });
    /* Missing or unplayable: hand the screen back to the drawing. */
    heroVid.addEventListener("error", function(){
      heroVid.style.opacity = "0";
      if (heroDrawn) heroDrawn.classList.remove("fade-out");
    });
    heroVid.src = "media/main.mp4";
    heroVid.load();
  }

  /* ------------------------------------------------------------ guide --- */

  var vid   = document.querySelector(".guide-screen");
  var gdev  = document.querySelector(".guide-stage .device");
  var gdrawn = document.querySelector(".guide-stage .device-screen-content");
  var steps = Array.prototype.slice.call(document.querySelectorAll(".step"));
  if (!vid || !steps.length) return;

  var current = null;

  vid.loop = true;
  vid.addEventListener("loadeddata", function(){
    vid.style.opacity = "1";
    var p = vid.play();
    if (p && p.catch) p.catch(function(){});
  });
  /* No clip recorded yet, or the file is missing: fall back to the drawn screen
     rather than leaving a black rectangle. */
  vid.addEventListener("error", function(){ vid.style.opacity = "0"; });

  function show(step){
    if (step === current) return;
    current = step;
    for (var i = 0; i < steps.length; i++) {
      steps[i].classList.toggle("is-active", steps[i] === step);
    }
    /* One step asks for the lid shut rather than a clip. Marked in the markup so
       renaming the step's heading cannot quietly break it. */
    if (gdev) {
      var wantShut = step.getAttribute("data-lid") === "shut";
      /* A closing lid turns the display off — that is what the step says. Leaving the
         drawn carousel up meant watching a mock-up of a running device fold shut, and it
         stays on the lid's front face all the way to 90 degrees, so it is visible for
         most of the swing. Dark is both correct and the point being made. */
      if (gdrawn) gdrawn.classList.toggle("fade-out", wantShut);
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
    if (!clip || reduced) { vid.style.opacity = "0"; vid.removeAttribute("src"); return; }
    /* Consecutive steps can name the same clip — the lid step closes on whatever the
       step before it was already playing. Re-setting an identical src would reload and
       restart it, so the picture would jump at the very moment the lid starts to swing.
       Leaving it alone keeps the game running underneath the close. */
    var path = "media/" + clip;
    if (vid.getAttribute("src") === path) return;
    vid.style.opacity = "0";
    vid.src = path;
    vid.load();
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
    document.querySelectorAll(".hero, .step, .faq, footer"));
  var dots = Array.prototype.slice.call(document.querySelectorAll(".dots a"));
  var ticking = false;
  var lastY = window.pageYOffset || 0;
  var dir = 1;                       /* 1 = moving down the page, -1 = moving up */

  function clearSteps(){
    for (var i = 0; i < steps.length; i++) steps[i].classList.remove("is-active");
    current = null;
    vid.style.opacity = "0";
    if (gdev) gdev.classList.remove("is-shut");
    if (gdrawn) gdrawn.classList.remove("fade-out");
  }

  function pick(){
    ticking = false;
    var y = window.pageYOffset || 0;
    if (y !== lastY) { dir = y > lastY ? 1 : -1; lastY = y; }
    var mid = window.innerHeight / 2, best = -1, bestDist = Infinity;
    for (var i = 0; i < sections.length; i++) {
      var r = sections[i].getBoundingClientRect();
      if (r.bottom < 0 || r.top > window.innerHeight) continue;
      var d = Math.abs((r.top + r.bottom) / 2 - mid);
      if (d < bestDist) { bestDist = d; best = i; }
    }
    if (best < 0) return;
    for (var k = 0; k < dots.length; k++) dots[k].classList.toggle("is-on", k === best);
    var sec = sections[best];
    if (sec.classList.contains("step")) show(sec); else clearSteps();
  }

  function onScroll(){
    if (!ticking) { ticking = true; requestAnimationFrame(pick); }
  }

  window.addEventListener("scroll", onScroll, { passive: true });
  window.addEventListener("resize", onScroll, { passive: true });
  pick();
})();
