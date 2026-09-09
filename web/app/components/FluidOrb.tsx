import { useEffect, useRef, useState } from 'react'

const VERTEX = `
attribute vec2 p;
void main() { gl_Position = vec4(p, 0.0, 1.0); }
`

/**
 * A domain-warped fbm flowing inside a circle: the ground colour at the top
 * fading into brand tan at the bottom, with a lit core carried along by the
 * flow so the form reads as an object with volume rather than a blurred disc.
 *
 * ADR 0008 bans background gradients, and this is the one exception it makes.
 * The line is shape versus surface: a full-bleed wash reads as dirt on the
 * page, a closed circle reads as something deliberately placed.
 */
const FRAGMENT = `
precision mediump float;
uniform vec2 u_res;
uniform float u_t;
uniform vec3 u_bg, u_top, u_mid, u_bot;

vec2 hash(vec2 p) {
  p = vec2(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)));
  return fract(sin(p) * 43758.5453) * 2.0 - 1.0;
}

float noise(vec2 p) {
  vec2 i = floor(p), f = fract(p);
  vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(
    mix(dot(hash(i), f), dot(hash(i + vec2(1, 0)), f - vec2(1, 0)), u.x),
    mix(dot(hash(i + vec2(0, 1)), f - vec2(0, 1)), dot(hash(i + vec2(1, 1)), f - vec2(1, 1)), u.x),
    u.y);
}

float fbm(vec2 p) {
  float v = 0.0, a = 0.5;
  for (int i = 0; i < 4; i++) { v += a * noise(p); p *= 2.03; a *= 0.5; }
  return v;
}

void main() {
  vec2 uv = gl_FragCoord.xy / u_res;
  float r = length(uv * 2.0 - 1.0);
  float t = u_t * 0.05;

  vec2 warp = vec2(fbm(uv * 2.3 + vec2(0.0, t)), fbm(uv * 2.3 + vec2(4.7, -t * 0.8)));
  float n = fbm(uv * 2.0 + warp * 0.9 + vec2(t * 0.35, -t * 0.22));
  float y = clamp(uv.y + n * 0.46, 0.0, 1.0);

  vec3 col = mix(u_bot, u_mid, smoothstep(0.04, 0.58, y));
  col = mix(col, u_top, smoothstep(0.50, 1.0, y));

  vec2 core = (uv - vec2(0.5 + warp.x * 0.12, 0.38 + warp.y * 0.12)) * 2.0;
  col = mix(col, u_bot, (1.0 - smoothstep(0.0, 0.9, length(core))) * 0.38);

  gl_FragColor = vec4(mix(u_bg, col, 1.0 - smoothstep(0.84, 1.0, r)), 1.0);
}
`

type Rgb = [number, number, number]

/** Reads a CSS colour through a 2D canvas, so any notation the browser accepts works. */
function readColor(probe: CanvasRenderingContext2D, value: string): Rgb {
  probe.fillStyle = '#000'
  probe.fillStyle = value
  const hex = probe.fillStyle as string
  if (!hex.startsWith('#') || hex.length !== 7) return [0, 0, 0]
  return [
    Number.parseInt(hex.slice(1, 3), 16) / 255,
    Number.parseInt(hex.slice(3, 5), 16) / 255,
    Number.parseInt(hex.slice(5, 7), 16) / 255,
  ]
}

function mix(a: Rgb, b: Rgb, amount: number): Rgb {
  return [
    a[0] + (b[0] - a[0]) * amount,
    a[1] + (b[1] - a[1]) * amount,
    a[2] + (b[2] - a[2]) * amount,
  ]
}

export function FluidOrb() {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  // The CSS hides the orb below 1080px. Tracking the same breakpoint here
  // means a phone never allocates a WebGL context or compiles a shader for a
  // canvas it will not show — and a window widened past it still gets one.
  const [wideEnough, setWideEnough] = useState(false)
  // No WebGL, or a shader that will not compile: the orb is decoration, so it
  // is dropped rather than degraded. Held as state so React removes the node.
  const [unsupported, setUnsupported] = useState(false)

  useEffect(() => {
    // Must match the `max-[1080px]:hidden` on the canvas exactly. Tailwind
    // compiles that to `not (min-width: 1080px)`, so 1080px itself is a width
    // where the canvas is visible — a 1081px guard left it blank there.
    const wide = window.matchMedia('(min-width: 1080px)')
    const sync = () => setWideEnough(wide.matches)
    sync()
    wide.addEventListener('change', sync)
    return () => wide.removeEventListener('change', sync)
  }, [])

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || !wideEnough || unsupported) return

    const gl = canvas.getContext('webgl', { antialias: false, alpha: false, depth: false })
    if (!gl) {
      setUnsupported(true)
      return
    }

    const compile = (type: number, source: string) => {
      const shader = gl.createShader(type)
      if (!shader) return null
      gl.shaderSource(shader, source)
      gl.compileShader(shader)
      return gl.getShaderParameter(shader, gl.COMPILE_STATUS) ? shader : null
    }

    const vertex = compile(gl.VERTEX_SHADER, VERTEX)
    const fragment = compile(gl.FRAGMENT_SHADER, FRAGMENT)
    const program = gl.createProgram()
    if (!vertex || !fragment || !program) {
      setUnsupported(true)
      return
    }
    gl.attachShader(program, vertex)
    gl.attachShader(program, fragment)
    gl.linkProgram(program)
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      setUnsupported(true)
      return
    }
    gl.useProgram(program)

    const buffer = gl.createBuffer()
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer)
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW)
    const attribute = gl.getAttribLocation(program, 'p')
    gl.enableVertexAttribArray(attribute)
    gl.vertexAttribPointer(attribute, 2, gl.FLOAT, false, 0, 0)

    const uniforms = {
      res: gl.getUniformLocation(program, 'u_res'),
      time: gl.getUniformLocation(program, 'u_t'),
      bg: gl.getUniformLocation(program, 'u_bg'),
      top: gl.getUniformLocation(program, 'u_top'),
      mid: gl.getUniformLocation(program, 'u_mid'),
      bot: gl.getUniformLocation(program, 'u_bot'),
    }

    const probeCanvas = document.createElement('canvas')
    const probe = probeCanvas.getContext('2d')

    const applyTone = () => {
      if (!probe) return
      const styles = getComputedStyle(document.documentElement)
      const paper = readColor(probe, styles.getPropertyValue('--paper').trim() || '#fff')
      const tan = readColor(probe, styles.getPropertyValue('--tan').trim() || '#dea584')
      const strength = Number.parseFloat(styles.getPropertyValue('--orb-strength')) || 0.46
      gl.uniform3fv(uniforms.bg, paper)
      gl.uniform3fv(uniforms.top, mix(paper, tan, strength * 0.06))
      gl.uniform3fv(uniforms.mid, mix(paper, tan, strength * 0.42))
      gl.uniform3fv(uniforms.bot, mix(paper, tan, strength))
    }

    const resize = () => {
      const box = canvas.getBoundingClientRect()
      const size = Math.round(Math.min(box.width, 420) * Math.min(devicePixelRatio, 2))
      if (size <= 0 || (canvas.width === size && canvas.height === size)) return
      canvas.width = size
      canvas.height = size
      gl.viewport(0, 0, size, size)
      gl.uniform2f(uniforms.res, size, size)
    }

    const draw = (seconds: number) => {
      gl.uniform1f(uniforms.time, seconds)
      gl.drawArrays(gl.TRIANGLES, 0, 3)
    }

    resize()
    applyTone()

    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)')
    let frame = 0
    let running = false
    // Tracked rather than re-read: `visibilitychange` must not restart the
    // loop for an orb that scrolled out of view while the tab was hidden.
    let onscreen = false
    const start = performance.now()

    const loop = () => {
      if (!running) return
      draw((performance.now() - start) / 1000)
      frame = requestAnimationFrame(loop)
    }

    const stop = () => {
      running = false
      cancelAnimationFrame(frame)
    }

    const play = () => {
      if (running || !onscreen || document.hidden) return
      if (reduce.matches) {
        draw(0)
        return
      }
      running = true
      frame = requestAnimationFrame(loop)
    }

    // Off screen or on a hidden tab, the loop is pure waste.
    const observer = new IntersectionObserver(([entry]) => {
      onscreen = entry?.isIntersecting ?? false
      if (onscreen) play()
      else stop()
    })
    observer.observe(canvas)

    const onVisibility = () => (document.hidden ? stop() : play())
    document.addEventListener('visibilitychange', onVisibility)

    // Turning reduced motion on mid-visit has to stop an animation already
    // running, and turning it off has to be allowed to start one.
    const onReduce = () => {
      if (reduce.matches) {
        stop()
        draw(0)
      } else {
        play()
      }
    }
    reduce.addEventListener('change', onReduce)

    const themeObserver = new MutationObserver(() => {
      applyTone()
      if (reduce.matches) draw(0)
    })
    themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })

    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)')
    const onScheme = () => {
      applyTone()
      if (reduce.matches) draw(0)
    }
    prefersDark.addEventListener('change', onScheme)

    const onResize = () => {
      resize()
      if (reduce.matches) draw(0)
    }
    window.addEventListener('resize', onResize)

    return () => {
      stop()
      // Crossing the breakpoint re-runs this effect on the same canvas, so the
      // previous program, shaders and buffer have to go back; otherwise a
      // window resized back and forth accumulates them on the GPU.
      gl.deleteBuffer(buffer)
      gl.deleteProgram(program)
      gl.deleteShader(vertex)
      gl.deleteShader(fragment)
      // Deliberately no `WEBGL_lose_context`: the canvas element outlives this
      // effect, and a context lost here stays lost. `getContext` would hand the
      // same dead context back on the way up past 1080px, `createShader` would
      // return null, and the orb would be gone for the rest of the visit.
      observer.disconnect()
      themeObserver.disconnect()
      document.removeEventListener('visibilitychange', onVisibility)
      reduce.removeEventListener('change', onReduce)
      prefersDark.removeEventListener('change', onScheme)
      window.removeEventListener('resize', onResize)
    }
  }, [wideEnough, unsupported])

  if (unsupported) return null

  // Hidden below 1080px: the hero text reaches the right margin there, and the
  // orb would sit behind it.
  return (
    <canvas
      ref={canvasRef}
      aria-hidden="true"
      className="pointer-events-none absolute top-[52%] right-2 z-0 aspect-square w-[clamp(220px,22vw,330px)] -translate-y-1/2 max-[1080px]:hidden"
    />
  )
}
