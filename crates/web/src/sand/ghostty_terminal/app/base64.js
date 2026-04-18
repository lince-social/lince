export function bytesToBase64(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length === 0) {
    return "";
  }

  let binary = "";
  const chunkSize = 0x8000;
  for (let start = 0; start < bytes.length; start += chunkSize) {
    const chunk = bytes.subarray(start, start + chunkSize);
    binary += String.fromCharCode(...chunk);
  }
  return btoa(binary);
}

export function base64ToBytes(value) {
  if (!value) {
    return new Uint8Array(0);
  }

  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}
