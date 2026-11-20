using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.IO.Hashing;
using System.Linq;
using System.Text;
using GRF.Core.GrfCompression;
using K4os.Compression.LZ4;
using Org.BouncyCastle.Crypto.Engines;
using Org.BouncyCastle.Crypto.Modes;
using Org.BouncyCastle.Crypto.Parameters;

// Makes jRO-protected entries (header 241, bodies 259/260/269) and checks every one with GRF
// Editor's own ProtectedGrfDecoder.Decode; writes them in the layout of Tests/fixtures/protected-grf.bin.
static class Program {
	static readonly Random Rng = new Random(20261120);
	static readonly byte[] HeaderKey = { 0x56, 0xFD, 0x7E, 0x71, 0x2F, 0x6D, 0x36, 0xF5, 0x5C, 0xA5, 0x99, 0xE6, 0x7B, 0x18, 0x10, 0x7A };
	static readonly byte[] HeaderIv = { 0x4C, 0xE8, 0xA1, 0xAE, 0x45, 0xB3, 0x21, 0x60 };

	sealed class V { public string Tag; public byte[] Name, Payload, Output; public int Size; }

	static byte[] Rand(int n) { var b = new byte[n]; Rng.NextBytes(b); return b; }
	static byte[] Slice(byte[] d, int o, int n) { var r = new byte[n]; Buffer.BlockCopy(d, o, r, 0, n); return r; }
	static uint BE(byte[] d, int o) => (uint)(d[o] << 24 | d[o + 1] << 16 | d[o + 2] << 8 | d[o + 3]);
	static void LE(byte[] d, int o, uint v) { d[o] = (byte)v; d[o + 1] = (byte)(v >> 8); d[o + 2] = (byte)(v >> 16); d[o + 3] = (byte)(v >> 24); }
	static void Xor(byte[] d, int o, int n, byte[] m) { for (int i = 0; i < n; i++) d[o + i] ^= m[i & 15]; }

	static byte[] Plain(int n) {
		var b = new byte[n];
		for (int i = 0; i < n; i++) b[i] = i % 3 == 0 ? (byte)(i / 11) : (byte)Rng.Next(256);
		return b;
	}

	static byte[] Compress(byte[] plain, int method) {
		if (method == 0) return (byte[])plain.Clone();
		if (method == 1) {
			var ms = new MemoryStream();
			using (var z = new ZLibStream(ms, CompressionLevel.Optimal, true)) z.Write(plain, 0, plain.Length);
			return ms.ToArray();
		}
		var dst = new byte[LZ4Codec.MaximumOutputSize(plain.Length)];
		int n = LZ4Codec.Encode(plain, 0, plain.Length, dst, 0, dst.Length);
		Array.Resize(ref dst, n);
		return dst;
	}

	static void CfbEncrypt(byte[] data, int off, int count, byte[] material, int keyAt, int ivAt, bool xtea) {
		uint[] key = new uint[4];
		for (int i = 0; i < 4; i++) key[i] = BE(material, keyAt + i * 4);
		byte[] fb = Slice(material, ivAt, 8), enc = new byte[8];
		for (int p = off; p < off + count; p += 8) {
			ProtectedJroDecoder.EncryptTeaBlock(key, fb, enc, xtea);
			for (int i = 0; i < 8; i++) data[p + i] ^= enc[i];
			Buffer.BlockCopy(data, p, fb, 0, 8);
		}
	}

	static V Make(string tag, ushort cipher, int method, int plainLen, bool flag, Action<byte[], byte[]> spoil = null) {
		byte[] name = Encoding.ASCII.GetBytes("data\\texture\\" + tag).Concat(new byte[] { 0xC0, 0xAF, 0xC0, 0xFA }).Concat(Encoding.ASCII.GetBytes(".bmp")).ToArray();
		byte[] plain = Plain(plainLen);
		byte[] comp = Compress(plain, method);
		int compressed = comp.Length;
		byte[] header = new byte[104];
		header[0] = 1; header[1] = 241; header[2] = (byte)Rng.Next(256); header[3] = (byte)Rng.Next(256);
		header[8] = (byte)cipher; header[9] = (byte)(cipher >> 8); header[10] = (byte)method; header[11] = (byte)(flag ? 1 : 0);
		LE(header, 16, (uint)compressed);
		LE(header, 20, (uint)plainLen);
		Buffer.BlockCopy(ProtectedGrfDecoder.NativeHash(plain), 0, header, 24, 16);
		Buffer.BlockCopy(Rand(60), 0, header, 44, 60);
		byte[] mat = Slice(header, 44, 60);
		if (flag) for (int i = 0; i < 60; i++) mat[i] ^= header[24 + (i & 15)];

		byte[] body;
		if (cipher == 269) {
			byte[] mask = ProtectedJroDecoder.DeriveMask(mat, header, name, 50, false);
			if (compressed <= 2176) {
				body = (byte[])comp.Clone();
				if (body.Length > 128) Xor(body, 128, body.Length - 128, mask);
			}
			else {
				int n = Math.Min(1024, compressed - 2176), pad = 8 - (n & 7);
				body = new byte[compressed + pad];
				Buffer.BlockCopy(comp, 0, body, 0, 2176);
				Xor(body, 128, 2048, mask);
				byte[] window = new byte[n + pad];
				Buffer.BlockCopy(comp, 2176, window, 0, n);
				for (int i = n; i < window.Length; i++) window[i] = (byte)pad;
				byte[] enc = ProtectedJroDecoder.TransformRc5Ofb(window, 0, window.Length, mat);
				Buffer.BlockCopy(enc, 0, body, 2176, enc.Length);
				int tail = compressed - 2176 - n;
				byte[] rest = Slice(comp, 2176 + n, tail);
				Xor(rest, 0, tail, mask);
				Buffer.BlockCopy(rest, 0, body, 2176 + n + pad, tail);
			}
		}
		else {
			body = new byte[compressed + Rng.Next(0, 12)];
			Buffer.BlockCopy(comp, 0, body, 0, compressed);
			for (int i = compressed; i < body.Length; i++) body[i] = (byte)Rng.Next(256);
			if (body.Length > 128) {
				int rest = body.Length - 128;
				if (cipher == 259) {
					Xor(body, 128, rest, ProtectedJroDecoder.DeriveMask(mat, header, name, 50, true));
					CfbEncrypt(body, 128, Math.Min(1024, rest & -8), mat, 0, 32, true);
				}
				else {
					bool ext = rest >= 3072;
					Xor(body, 128, rest, ProtectedJroDecoder.DeriveMask(mat, header, name, 48, false));
					CfbEncrypt(body, ext ? 2176 : 128, ext ? 1024 : rest & ~7, mat, 32, 0, false);
				}
			}
		}
		LE(header, 12, (uint)body.Length);
		spoil?.Invoke(header, body);
		LE(header, 4, XxHash32.HashToUInt32(Slice(header, 8, 96)));
		var cbc = new CbcBlockCipher(new IdeaEngine());
		cbc.Init(true, new ParametersWithIV(new KeyParameter(HeaderKey), HeaderIv));
		byte[] encHeader = (byte[])header.Clone();
		for (int o = 8; o < 104; o += 8) cbc.ProcessBlock(header, o, encHeader, o);
		byte[] payload = encHeader.Concat(body).ToArray();
		return new V { Tag = tag, Name = name, Payload = payload, Output = plain, Size = plainLen };
	}

	static int Main(string[] args) {
		var keys = ProtectedGrfDecoder.DecodeArchiveKeys(new byte[0]);
		var vectors = new List<V>();
		int ok = 0, bad = 0;
		foreach (ushort cipher in new ushort[] { 259, 260, 269 }) {
			foreach (var (method, len) in new[] { (0, 100), (0, 129), (0, 1000), (0, 2176), (0, 3000), (0, 6000), (1, 4000), (2, 9000) }) {
				var v = Make($"jro-ok-{cipher}-m{method}-{len}", cipher, method, len, len % 2 == 0);
				byte[] got = ProtectedGrfDecoder.Decode(v.Payload, v.Name, v.Size, keys);
				if (!got.SequenceEqual(v.Output)) throw new Exception("GRF Editor decodes " + v.Tag + " differently");
				vectors.Add(v); ok++;
			}
		}
		var spoilt = new List<V> {
			Make("jro-bad-checksum", 259, 0, 700, false),
			Make("jro-bad-hash", 260, 0, 700, true, (h, b) => b[5] ^= 1),
			Make("jro-bad-269-length", 269, 0, 1500, false, (h, b) => LE(h, 16, 1499)),
			Make("jro-bad-269-padding", 269, 0, 5000, false, (h, b) => b[2176 + 1024 + 3] ^= 0x55),
			Make("jro-bad-cipher", 261, 0, 700, false),
		};
		spoilt[0].Payload[40] ^= 1;
		foreach (var v in spoilt) {
			try {
				ProtectedGrfDecoder.Decode(v.Payload, v.Name, v.Size, keys);
				throw new Exception("GRF Editor accepts " + v.Tag);
			}
			catch (InvalidDataException e) { Console.WriteLine($"{v.Tag}: {e.Message}"); }
			v.Output = new byte[0];
			vectors.Add(v); bad++;
		}
		using (var w = new BinaryWriter(File.Create(args[0]))) {
			w.Write(vectors.Count);
			foreach (var v in vectors) {
				foreach (var f in new[] { Encoding.UTF8.GetBytes(v.Tag), v.Name, new byte[0], v.Payload, v.Output }) { w.Write(f.Length); w.Write(f); }
				w.Write(v.Size);
			}
		}
		Console.WriteLine($"PASS {ok} decoded by GRF Editor, {bad} rejected by it -> {args[0]}");
		return 0;
	}
}
