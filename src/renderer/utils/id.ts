export function createStableId(): string {
  const cryptoApi = globalThis.crypto
  if (cryptoApi?.randomUUID) {
    return cryptoApi.randomUUID()
  }

  if (cryptoApi?.getRandomValues) {
    const bytes = new Uint8Array(16)
    cryptoApi.getRandomValues(bytes)
    // RFC 4122 v4 variant bits.
    bytes[6] = (bytes[6] & 0x0f) | 0x40
    bytes[8] = (bytes[8] & 0x3f) | 0x80
    const hex = [...bytes].map(v => v.toString(16).padStart(2, "0")).join("")
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
  }

  return `id-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
}
