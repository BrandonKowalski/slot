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
    var heroDevEl = document.querySelector(".hero-device .device");
    if (heroDevEl) heroDevEl.className = "device pose-" + pose;
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
  var gdev  = document.querySelector(".guide-stage .device");
  var steps = Array.prototype.slice.call(document.querySelectorAll(".step"));

  var current = null;

  /* The lid's transition in the stylesheet. Kept in step by hand: a swing that outlasts this
     shows the gap again at its tail. */
  var LID_SWING_MS = 1150;
  var swingTimer = null;

  var playRetries = [];
  function ensurePlaying(){
    if (reduced || !vid || !vid.getAttribute("src") || !vid.paused) return;
    var p = vid.play();
    if (p && p.catch) p.catch(function(){});
  }
  function nudgePlayback(){
    for (var i = 0; i < playRetries.length; i++) clearTimeout(playRetries[i]);
    playRetries = [];
    ensurePlaying();
    /* And again shortly after, for the cases no event covers: a play aborted by the next
       load, or an element that had no box when it was first asked. */
    playRetries.push(setTimeout(ensurePlaying, 220), setTimeout(ensurePlaying, 800));
  }
  if (vid) {
    vid.loop = true;
    /* Asked again on every signal that the element might now be able to start. */
    vid.addEventListener("loadeddata", ensurePlaying);
    vid.addEventListener("canplay", ensurePlaying);
    vid.addEventListener("canplaythrough", ensurePlaying);
  }
  /* Not hidden on error: the poster is a separate file and a real frame of the step, so a
     clip that is missing or unplayable still leaves the right picture on the screen. */

  /* Which step the pinned screen is showing. Called by pressing a step rather than by
     scrolling onto one, which is the only thing about it that changed: the lid, the clip
     and the active row are decided here exactly as they were. */
  function show(step){
    if (!vid || !step || step === current) return;
    current = step;
    var at = 0;
    for (var i = 0; i < steps.length; i++) {
      var on = steps[i] === step;
      steps[i].classList.toggle("is-active", on);
      if (on) at = i;
    }
    if (stepAt) stepAt.textContent = String(at + 1);
    /* The ends are dead rather than wrapping: six steps in an order, not a carousel, so
       arriving back at the first one from the last would lose your place in it. */
    if (prevBtn) prevBtn.disabled = at === 0;
    if (nextBtn) nextBtn.disabled = at === steps.length - 1;
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
      gdev.classList.toggle("is-shut", wantShut);
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
       restart it, so the picture would jump at the very moment the lid starts to swing. */
    var path = "media/" + clip;
    if (vid.getAttribute("src") === path) {
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

  /* Stepped through rather than listed, so the list only ever shows the step being read. The
     class is what switches the stylesheet from its no-script fallback to that. */
  var stepList = document.querySelector(".guide-steps");
  var prevBtn  = document.getElementById("step-prev");
  var nextBtn  = document.getElementById("step-next");
  var stepAt   = document.getElementById("step-at");
  var stepOf   = document.getElementById("step-of");
  if (stepList && steps.length) stepList.classList.add("is-live");
  if (stepOf) stepOf.textContent = String(steps.length);

  function stepBy(delta){
    var at = steps.indexOf(current);
    var to = at + delta;
    if (at < 0 || to < 0 || to >= steps.length) return;
    show(steps[to]);
    setHash(steps[to].id);
  }
  if (prevBtn) prevBtn.addEventListener("click", function(){ stepBy(-1); });
  if (nextBtn) nextBtn.addEventListener("click", function(){ stepBy(1); });

  /* The arrows work anywhere on the guide that is not a tab row, which has its own. */
  var guidePanel = document.getElementById("guide");
  if (guidePanel) guidePanel.addEventListener("keydown", function(e){
    if (e.target.closest("[role=tablist]")) return;
    if (e.key === "ArrowLeft") { e.preventDefault(); stepBy(-1); }
    else if (e.key === "ArrowRight") { e.preventDefault(); stepBy(1); }
  });

  /* ------------------------------------------------------------- tabs --- */

  /* One routine drives both rows. A tablist is its buttons plus the panels they name, and
     nothing else distinguishes the section tabs from the sub-tabs inside Buttons and
     Questions, so they share the code and adding a row is adding markup. */
  function wire(list){
    var tabs = Array.prototype.slice.call(list.querySelectorAll("[role=tab]"));
    var group = {
      tabs: tabs,
      panels: tabs.map(function(t){ return document.getElementById(t.getAttribute("aria-controls")); }),
      select: function(tab, focus){
        var i = tabs.indexOf(tab);
        if (i < 0) return;
        for (var k = 0; k < tabs.length; k++) {
          var on = k === i;
          tabs[k].setAttribute("aria-selected", on ? "true" : "false");
          /* Only the selected tab is in the tab order; the arrows move within the row.
             That is the roving tabindex the ARIA pattern asks for, and it is what stops a
             six-tab row costing six presses to step over. */
          tabs[k].tabIndex = on ? 0 : -1;
          if (group.panels[k]) group.panels[k].hidden = !on;
        }
        if (hero) hero.hidden = true;
        /* The strip scrolls on a phone, so the tab that was just chosen is brought into view
           rather than left off the edge. `nearest` on the block axis so bringing a tab into
           view sideways cannot also scroll the page up or down under the reader. */
        if (tabs[i].scrollIntoView && list.scrollWidth > list.clientWidth + 1) {
          try { tabs[i].scrollIntoView({ inline: "center", block: "nearest" }); } catch (e) {}
        }
        if (focus) tabs[i].focus();
      }
    };
    for (var i = 0; i < tabs.length; i++) {
      (function(tab){
        tab.addEventListener("click", function(){
          group.select(tab, false);
          setHash(tab.getAttribute("aria-controls"));
        });
      })(tabs[i]);
    }
    list.addEventListener("keydown", function(e){
      var at = tabs.indexOf(document.activeElement);
      if (at < 0) return;
      var to = -1;
      if (e.key === "ArrowRight" || e.key === "ArrowDown") to = (at + 1) % tabs.length;
      else if (e.key === "ArrowLeft" || e.key === "ArrowUp") to = (at - 1 + tabs.length) % tabs.length;
      else if (e.key === "Home") to = 0;
      else if (e.key === "End") to = tabs.length - 1;
      if (to < 0) return;
      e.preventDefault();
      group.select(tabs[to], true);
      setHash(tabs[to].getAttribute("aria-controls"));
    });
    return group;
  }

  var hero = document.getElementById("hero");
  var groups = Array.prototype.slice.call(document.querySelectorAll("[role=tablist]")).map(wire);

  /* ------------------------------------------------------- phone menu --- */

  /* Six labels come to 448px against 229px of masthead on the narrowest phone, so side by
     side they are a row and narrow they are a list behind a button. The stylesheet decides
     which; this only opens and shuts it, and the button is not on screen at all at the width
     where the row fits. */
  var menuBtn = document.getElementById("menu-btn");
  var tabsNav = document.getElementById("section-tabs");
  function setMenu(open){
    if (!menuBtn || !tabsNav) return;
    tabsNav.classList.toggle("is-open", open);
    menuBtn.setAttribute("aria-expanded", open ? "true" : "false");
  }
  if (menuBtn && tabsNav) {
    menuBtn.addEventListener("click", function(e){
      e.stopPropagation();
      setMenu(!tabsNav.classList.contains("is-open"));
    });
    /* Choosing a section is the end of choosing one. */
    tabsNav.addEventListener("click", function(){ setMenu(false); });
    /* Anywhere else puts it away, which is what a menu over the page should do rather than
       waiting to be dismissed by the control that opened it. */
    document.addEventListener("click", function(e){
      if (e.target !== menuBtn && !menuBtn.contains(e.target) && !tabsNav.contains(e.target)) {
        setMenu(false);
      }
    });
    document.addEventListener("keydown", function(e){
      if (e.key !== "Escape" || !tabsNav.classList.contains("is-open")) return;
      setMenu(false);
      menuBtn.focus();
    });
  }

  /* Back to the landing. Every tab is deselected rather than one being left lit over a panel
     that is no longer up, and the first one keeps the tab stop so the row is still reachable
     from the keyboard with nothing in it chosen. */
  function showHero(){
    /* The section row only. The rows inside Buttons and Questions own a choice that belongs to
       their own panel, and clearing those too left the sub-panel on screen with nothing in its
       row marked: coming back to Buttons after a trip through the hero showed the Everywhere
       list under a row where Everywhere was no longer underlined. */
    var grp = groups[0];
    if (!grp) return;
    for (var k = 0; k < grp.tabs.length; k++) {
      grp.tabs[k].setAttribute("aria-selected", "false");
      grp.tabs[k].tabIndex = k === 0 ? 0 : -1;
    }
    for (var q = 0; q < grp.panels.length; q++) {
      if (grp.panels[q]) grp.panels[q].hidden = true;
    }
    if (hero) hero.hidden = false;
  }

  var mark = document.querySelector(".mast-mark");
  if (mark) mark.addEventListener("click", function(e){
    e.preventDefault();
    showHero();
    setHash("hero");
  });

  /* ---------------------------------------------------------- routing --- */

  /* Every id that used to be a section is still an id on this page: the four button lists,
     the four question lists, the six steps and both endnotes. So a published link like
     #controls-in-game needs no table to translate it. Resolve the id to its element and walk
     up: whatever tablist owns it gets selected, at every level, and a step selects itself.
     Adding a panel adds nothing here. */
  var routing = false;
  function setHash(id){
    if (!id) return;
    routing = true;
    if (history.replaceState) history.replaceState(null, "", "#" + id);
    else location.hash = id;
    routing = false;
  }

  function reveal(id){
    var el = id && document.getElementById(id);
    if (!el) return false;
    if (el === hero) { showHero(); return true; }
    /* Select from the outside in, so a panel is already visible when the row inside it is
       asked to choose. */
    var chain = [];
    for (var node = el; node && node !== document.body; node = node.parentNode) {
      for (var g = 0; g < groups.length; g++) {
        var at = groups[g].panels.indexOf(node);
        if (at >= 0) chain.unshift({ group: groups[g], tab: groups[g].tabs[at] });
      }
    }
    for (var c = 0; c < chain.length; c++) chain[c].group.select(chain[c].tab, false);
    if (el.classList.contains("step")) show(el);
    return true;
  }

  window.addEventListener("hashchange", function(){
    if (routing) return;
    reveal(location.hash.slice(1));
  });

  /* The guide opens on its first step whichever way it was reached, so the pinned screen is
     never blank. Done before the hash is honoured, so a link straight to a later step wins. */
  if (steps.length) show(steps[0]);
  /* A hash naming nothing leaves the markup's own selection alone, which is the hero. */
  reveal(location.hash.slice(1));

})();
