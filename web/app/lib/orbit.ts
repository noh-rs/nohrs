/**
 * The arithmetic behind the hero ring, kept out of the component so it can be
 * checked without a browser. Both rules are easy to get subtly wrong and
 * impossible to see going wrong: the first shows up only as a panel taking the
 * long way round when it opens, the second as a panel framed slightly off.
 */

/**
 * The angle of the panel at `index`, clockwise from the top and folded into
 * (-180, 180]. A panel opens by rotating back to zero, and the fold is what
 * sends the last one in the ring 72° back rather than 288° forward.
 */
export function angleOf(index: number, count: number): number {
  const degrees = ((index * 360) / count) % 360
  return degrees > 180 ? degrees - 360 : degrees
}

/**
 * The `transform-origin` that frames `centre` (0–1 across the shot) when the
 * shot is scaled by `zoom` inside a panel of its own proportions.
 *
 * Scaling holds the origin still rather than centring on it, so the two only
 * coincide in the middle of the image. Solving for the origin that puts
 * `centre` in the middle of what remains visible gives the expression below.
 *
 * `spill` is how far the frame may hang past the *start* of the image, as a
 * fraction of the frame's own size, and only the vertical axis asks for any.
 * Every panel is turned to face outward and cut by the edge it faces, so its
 * top is never on the page and a strip of nothing there is never seen. Without
 * that allowance a panel can only frame the lower half of a window whose
 * content — the toolbar, the listing, the pane beside it — is all at the top.
 */
export function originFor(centre: number, zoom: number, spill = 0): number {
  if (!(zoom > 1)) return 0.5
  const half = 0.5 / zoom
  const inside = Math.min(Math.max(centre, half - spill / zoom), 1 - half)
  return (inside - half) / (1 - 1 / zoom)
}

/**
 * A frame that is not the shot's shape shows a slice of it. `fraction` is how
 * much of the axis survives — the number `object-fit: cover` arrives at, the
 * ratio of the two aspects — and this is the `object-position` that centres
 * `centre` in that slice, held far enough in to keep the frame filled.
 *
 * It is `originFor` again: holding a slice `fraction` wide against a point is
 * the same problem as holding a whole frame against it under a zoom of
 * `1 / fraction`.
 */
export function cropFor(centre: number, fraction: number): number {
  return originFor(centre, 1 / fraction)
}

/**
 * Where `centre` ends up inside the frame once that slice is taken — the middle
 * of it, unless the slice had to be held off an edge. It is the point the
 * opened shot has to unzoom from, which is no longer the point on the shot.
 */
export function centreOfCrop(centre: number, fraction: number): number {
  if (fraction >= 1) return centre
  return (centre - cropFor(centre, fraction) * (1 - fraction)) / fraction
}
