import { describe, expect, it } from 'vitest'
import { fileNameFromPath } from './attachments'

describe('fileNameFromPath', () => {
  it('extracts names from Linux and Windows file paths', () => {
    expect(fileNameFromPath('/home/user/单据.jpg')).toBe('单据.jpg')
    expect(fileNameFromPath('C:\\Users\\user\\receipt.png')).toBe('receipt.png')
  })
})
