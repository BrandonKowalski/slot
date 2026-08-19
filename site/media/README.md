# Clips

One clip per guide step, plus `main.mp4` for the hero. Which step plays which is set by
`data-clip` on the `.step` in `index.html`; the hero's is named in `app.js`.

**Record at 720x480** — the panel's native size, and exactly 3x the GBA's 240x160. The
screen box is 3:2 and the video is `object-fit:fill`, so a clip at any other aspect is
stretched rather than cropped.

## Every clip needs a still beside it

`carousel.mp4` needs `carousel.webp`, and so on. The still is the video's `poster`: it is
what the screen shows while the clip is loading, what it keeps showing if the clip never
arrives, and what it shows instead of the clip under `prefers-reduced-motion`. Nothing
falls back to a drawing any more, so a missing still means a black panel.

The still is named twice in `index.html` and the two have to agree: `data-clip` on the
`<article class="step">` names the clip, and the `poster` on that step's own `<video>`
names its still. The poster is written out rather than derived so the stills still show
with scripting off. Renaming a clip means editing both.

`step-sleep` has a `data-clip` but no `<video>` of its own: side by side the pinned device
plays that clip behind a closing lid, and stacked the panel is simply dark, because the
step is about the display going off.

Use frame 0, so the poster is exactly the frame the clip starts on and there is no jump
when playback begins:

```sh
for f in *.mp4; do
  ffmpeg -y -i "$f" -vf "select=eq(n\,0)" -vframes 1 -c:v libwebp -quality 82 "${f%.mp4}.webp"
done
```

If a clip's first frame is black or a fade-in, pick a later one (`select=eq(n\,30)`) —
the poster should show the thing the step is about.
