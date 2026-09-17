import { describe, expect, it } from 'vitest'
import { withDatabaseAccess, withDatabaseRead } from './database'

function deferred() {
  let resolve!: () => void
  const promise = new Promise<void>((done) => { resolve = done })
  return { promise, resolve }
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('database reader/writer gate', () => {
  it('allows independent reads to run concurrently', async () => {
    const hold = deferred()
    let secondStarted = false
    const first = withDatabaseRead(() => hold.promise)
    const second = withDatabaseRead(async () => { secondStarted = true })

    await tick()
    expect(secondStarted).toBe(true)
    hold.resolve()
    await Promise.all([first, second])
  })

  it('keeps writes exclusive and does not let later reads jump the queue', async () => {
    const holdRead = deferred()
    const holdWrite = deferred()
    let writeStarted = false
    let laterReadStarted = false
    const firstRead = withDatabaseRead(() => holdRead.promise)
    const write = withDatabaseAccess(async () => {
      writeStarted = true
      await holdWrite.promise
    })
    const laterRead = withDatabaseRead(async () => { laterReadStarted = true })

    await tick()
    expect(writeStarted).toBe(false)
    expect(laterReadStarted).toBe(false)
    holdRead.resolve()
    await tick()
    expect(writeStarted).toBe(true)
    expect(laterReadStarted).toBe(false)
    holdWrite.resolve()
    await Promise.all([firstRead, write, laterRead])
    expect(laterReadStarted).toBe(true)
  })
})
