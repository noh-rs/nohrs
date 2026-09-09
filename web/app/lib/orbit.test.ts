import assert from 'node:assert/strict'
import { test } from 'node:test'
import { angleOf, originFor } from './orbit.ts'

/** What the panel actually shows: the band of the shot left inside it. */
function framed(centre: number, zoom: number): [number, number] {
  const origin = originFor(centre, zoom)
  const start = origin * (1 - 1 / zoom)
  return [start, start + 1 / zoom]
}

test('angles run clockwise from the top', () => {
  assert.equal(angleOf(0, 5), 0)
  assert.equal(angleOf(1, 5), 72)
  assert.equal(angleOf(2, 5), 144)
})

test('the far side of the ring turns back rather than on round', () => {
  assert.equal(angleOf(3, 5), -144)
  assert.equal(angleOf(4, 5), -72)
  for (let index = 0; index < 12; index += 1) {
    assert.ok(Math.abs(angleOf(index, 12)) <= 180)
  }
})

test('a panel opposite the top may turn either way, but only half a turn', () => {
  assert.equal(angleOf(3, 6), 180)
})

test('the framed centre comes back centred', () => {
  for (const zoom of [1.5, 2, 2.4, 3]) {
    for (const centre of [0.35, 0.5, 0.62]) {
      const [start, end] = framed(centre, zoom)
      assert.ok(Math.abs((start + end) / 2 - centre) < 1e-9, `${centre} at ${zoom}`)
    }
  }
})

test('a centre near an edge is pulled in far enough to keep the frame filled', () => {
  for (const zoom of [1.5, 2, 3]) {
    for (const centre of [-1, 0, 0.02, 0.98, 1, 2]) {
      const [start, end] = framed(centre, zoom)
      assert.ok(start >= -1e-9 && end <= 1 + 1e-9, `${centre} at ${zoom} → ${start}..${end}`)
    }
  }
})

test('a shot that is not zoomed is framed whole', () => {
  assert.equal(originFor(0.2, 1), 0.5)
  assert.deepEqual(framed(0.2, 1), [0, 1])
})
