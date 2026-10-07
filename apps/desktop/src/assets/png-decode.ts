import { inflateSync } from "node:zlib";

// Décodeur PNG minimal pour les tests : 8 bits, RGBA (type 6) ou RGB (type 2), sans entrelacement,
// ce que produit le canvas de `scripts/build-icons.mjs`. Rend les pixels en RGBA.
export function decodePng(buffer: Buffer): { width: number; height: number; rgba: Uint8Array } {
  if (buffer.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a") throw new Error("pas un PNG");
  let width = 0;
  let height = 0;
  let channels = 0;
  const data: Buffer[] = [];
  for (let at = 8; at < buffer.length; ) {
    const length = buffer.readUInt32BE(at);
    const type = buffer.subarray(at + 4, at + 8).toString("ascii");
    const body = buffer.subarray(at + 8, at + 8 + length);
    if (type === "IHDR") {
      width = body.readUInt32BE(0);
      height = body.readUInt32BE(4);
      if (body[8] !== 8 || body[12] !== 0)
        throw new Error("PNG non géré (profondeur, entrelacement)");
      if (body[9] === 6) channels = 4;
      else if (body[9] === 2) channels = 3;
      else throw new Error(`type de couleur PNG non géré : ${body[9]}`);
    } else if (type === "IDAT") data.push(body);
    at += 12 + length;
  }
  const raw = inflateSync(Buffer.concat(data));
  const stride = width * channels;
  const out = new Uint8Array(width * height * 4);
  let previous = new Uint8Array(stride);
  for (let y = 0; y < height; y += 1) {
    const filter = raw[y * (stride + 1)] ?? 0;
    const line = new Uint8Array(raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)));
    for (let i = 0; i < stride; i += 1) {
      const a = i >= channels ? (line[i - channels] ?? 0) : 0;
      const b = previous[i] ?? 0;
      const c = i >= channels ? (previous[i - channels] ?? 0) : 0;
      let add = 0;
      if (filter === 1) add = a;
      else if (filter === 2) add = b;
      else if (filter === 3) add = (a + b) >> 1;
      else if (filter === 4) {
        const p = a + b - c;
        const [pa, pb, pc] = [Math.abs(p - a), Math.abs(p - b), Math.abs(p - c)];
        add = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      line[i] = ((line[i] ?? 0) + add) & 255;
    }
    for (let x = 0; x < width; x += 1) {
      const o = (y * width + x) * 4;
      out[o] = line[x * channels] ?? 0;
      out[o + 1] = line[x * channels + 1] ?? 0;
      out[o + 2] = line[x * channels + 2] ?? 0;
      out[o + 3] = channels === 4 ? (line[x * channels + 3] ?? 0) : 255;
    }
    previous = line;
  }
  return { width, height, rgba: out };
}
