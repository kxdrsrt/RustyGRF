"use strict";

// A small WebGL2 viewer for textured triangle meshes: orbit with the mouse, zoom with the wheel,
// double-click resets. No library: the backend sends positions, normals and uvs as base64 floats.

const Viewer3D = (() => {
  const VS = `#version 300 es
in vec3 aPos; in vec3 aNormal; in vec2 aUv;
uniform mat4 uMvp; uniform mat4 uModel; uniform mat3 uNormalMat;
out vec3 vNormal; out vec2 vUv;
void main() { vNormal = uNormalMat * mat3(uModel) * aNormal; vUv = aUv; gl_Position = uMvp * uModel * vec4(aPos, 1.0); }`;
  const FS = `#version 300 es
precision mediump float;
in vec3 vNormal; in vec2 vUv;
uniform sampler2D uTex; uniform bool uHasTex; uniform bool uLit; uniform vec3 uLight; uniform float uAlpha; uniform vec3 uFlat;
out vec4 outColor;
void main() {
  vec4 c = uHasTex ? texture(uTex, vUv) : vec4(uFlat, 1.0);
  if (c.a < 0.5) discard;
  float l = uLit ? 0.45 + 0.55 * max(dot(normalize(vNormal), uLight), 0.0) : 1.0;
  outColor = vec4(c.rgb * l, c.a * uAlpha);
}`;

  const mat = {
    perspective(fovy, aspect, near, far) {
      const f = 1 / Math.tan(fovy / 2), nf = 1 / (near - far);
      return [f / aspect, 0, 0, 0, 0, f, 0, 0, 0, 0, (far + near) * nf, -1, 0, 0, 2 * far * near * nf, 0];
    },
    lookAt(e, c, u) {
      let z = [e[0] - c[0], e[1] - c[1], e[2] - c[2]];
      const zl = Math.hypot(...z) || 1; z = z.map((v) => v / zl);
      let x = [u[1] * z[2] - u[2] * z[1], u[2] * z[0] - u[0] * z[2], u[0] * z[1] - u[1] * z[0]];
      const xl = Math.hypot(...x) || 1; x = x.map((v) => v / xl);
      const y = [z[1] * x[2] - z[2] * x[1], z[2] * x[0] - z[0] * x[2], z[0] * x[1] - z[1] * x[0]];
      return [x[0], y[0], z[0], 0, x[1], y[1], z[1], 0, x[2], y[2], z[2], 0, -(x[0] * e[0] + x[1] * e[1] + x[2] * e[2]), -(y[0] * e[0] + y[1] * e[1] + y[2] * e[2]), -(z[0] * e[0] + z[1] * e[1] + z[2] * e[2]), 1];
    },
    mul(a, b) {
      const o = new Array(16).fill(0);
      for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) for (let k = 0; k < 4; k++) o[c * 4 + r] += a[k * 4 + r] * b[c * 4 + k];
      return o;
    },
  };

  const f32 = (b64) => {
    const bin = atob(b64);
    const u = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) u[i] = bin.charCodeAt(i);
    return new Float32Array(u.buffer);
  };

  function shader(gl, type, src) {
    const s = gl.createShader(type);
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s));
    return s;
  }

  /** Mouse and keyboard control of a camera that turns around `target`: drag = turn, shift/right/middle drag or arrows = move, wheel = zoom, double click = reset. */
  function orbit(canvas, cam, home, radius, draw) {
    let drag = null;
    canvas.tabIndex = 0;
    canvas.addEventListener("contextmenu", (e) => e.preventDefault());
    canvas.addEventListener("pointerdown", (e) => { drag = { x: e.clientX, y: e.clientY, pan: e.shiftKey || e.button !== 0 }; canvas.setPointerCapture(e.pointerId); canvas.focus(); });
    canvas.addEventListener("pointerup", () => (drag = null));
    canvas.addEventListener("pointermove", (e) => {
      if (!drag) return;
      const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
      if (drag.pan) movePan(-dx, dy);
      else { cam.yaw -= dx * 0.01; cam.pitch = Math.max(-1.5, Math.min(1.5, cam.pitch + dy * 0.01)); }
      drag = { ...drag, x: e.clientX, y: e.clientY };
      draw();
    });
    function movePan(right, up) {
      const k = cam.dist * 0.0016;
      cam.target[0] += Math.cos(cam.yaw) * right * k;
      cam.target[2] += -Math.sin(cam.yaw) * right * k;
      cam.target[1] += up * k;
    }
    function walk(forward, side) {
      const k = cam.dist * 0.04;
      const sy = Math.sin(cam.yaw), cy = Math.cos(cam.yaw);
      cam.target[0] += (-sy * forward + cy * side) * k;
      cam.target[2] += (-cy * forward - sy * side) * k;
    }
    canvas.addEventListener("keydown", (e) => {
      const map = { ArrowUp: [1, 0], ArrowDown: [-1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1], w: [1, 0], s: [-1, 0], a: [0, -1], d: [0, 1] };
      const m = map[e.key];
      if (!m) return;
      e.preventDefault();
      walk(m[0], m[1]);
      draw();
    });
    canvas.addEventListener("wheel", (e) => { e.preventDefault(); cam.dist = Math.max(radius * 0.05, Math.min(radius * 30, cam.dist * Math.exp(e.deltaY * 0.001))); draw(); }, { passive: false });
    canvas.addEventListener("dblclick", () => { Object.assign(cam, { yaw: home.yaw, pitch: home.pitch, dist: home.dist, target: [...home.target] }); draw(); });
  }

  function eyeOf(cam) {
    return [cam.target[0] + cam.dist * Math.cos(cam.pitch) * Math.sin(cam.yaw), cam.target[1] + cam.dist * Math.sin(cam.pitch), cam.target[2] + cam.dist * Math.cos(cam.pitch) * Math.cos(cam.yaw)];
  }

  function makeTexture(gl, png, onLoad, mime = "image/png") {
    if (!png) return null;
    const tex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([128, 128, 128, 255]));
    const img = new Image();
    img.onload = () => {
      gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, img);
      gl.generateMipmap(gl.TEXTURE_2D);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.REPEAT);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.REPEAT);
      onLoad();
    };
    img.src = `data:${mime};base64,${png}`;
    return tex;
  }

  function program(gl, vs, fs) {
    const p = gl.createProgram();
    gl.attachShader(p, shader(gl, gl.VERTEX_SHADER, vs));
    gl.attachShader(p, shader(gl, gl.FRAGMENT_SHADER, fs));
    gl.linkProgram(p);
    if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p));
    return p;
  }

  /** Shows `model` (the backend's `model` preview) in a new canvas inside `host`; returns a handle with `destroy()`. */
  function show(host, model) {
    const canvas = document.createElement("canvas");
    canvas.className = "gl";
    host.append(canvas);
    const gl = canvas.getContext("webgl2", { antialias: true, alpha: false });
    if (!gl) {
      canvas.replaceWith(Object.assign(document.createElement("p"), { className: "error", textContent: "WebGL 2 is not available in this window." }));
      return { destroy() {} };
    }
    const prog = gl.createProgram();
    gl.attachShader(prog, shader(gl, gl.VERTEX_SHADER, VS));
    gl.attachShader(prog, shader(gl, gl.FRAGMENT_SHADER, FS));
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(prog));
    const loc = (n) => gl.getUniformLocation(prog, n);
    const U = { mvp: loc("uMvp"), model: loc("uModel"), nm: loc("uNormalMat"), tex: loc("uTex"), has: loc("uHasTex"), lit: loc("uLit"), light: loc("uLight"), alpha: loc("uAlpha"), flat: loc("uFlat") };

    const textures = model.textures.map((t) => {
      if (!t.png) return null;
      const tex = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([128, 128, 128, 255]));
      const img = new Image();
      img.onload = () => {
        gl.bindTexture(gl.TEXTURE_2D, tex);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, img);
        gl.generateMipmap(gl.TEXTURE_2D);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.REPEAT);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.REPEAT);
        draw();
      };
      img.src = `data:image/png;base64,${t.png}`;
      return tex;
    });

    const nodes = model.nodes.map((node) => ({ matrices: f32(node.matrices), parts: node.parts.map(makePart) }));
    function makePart(p) {
      const vao = gl.createVertexArray();
      gl.bindVertexArray(vao);
      const attr = (name, data, size) => {
        const b = gl.createBuffer();
        gl.bindBuffer(gl.ARRAY_BUFFER, b);
        gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
        const l = gl.getAttribLocation(prog, name);
        gl.enableVertexAttribArray(l);
        gl.vertexAttribPointer(l, size, gl.FLOAT, false, 0, 0);
      };
      const pos = f32(p.positions);
      attr("aPos", pos, 3);
      attr("aNormal", f32(p.normals), 3);
      attr("aUv", f32(p.uvs), 2);
      return { vao, count: pos.length / 3, texture: textures[p.texture] || null };
    }

    const c = [0, 1, 2].map((i) => (model.min[i] + model.max[i]) / 2);
    const radius = Math.max(1, Math.hypot(model.max[0] - model.min[0], model.max[1] - model.min[1], model.max[2] - model.min[2]) / 2);
    const cam = { yaw: 0.6, pitch: 0.35, dist: radius * 2.6, target: [...c] };
    const opts = { lit: true, wire: false, animate: true };
    const moving = model.duration_ms > 0 && model.samples > 1;
    let dirty = false;
    let timer = null;
    const clock = performance.now();
    const sampleNow = () => (moving && opts.animate ? Math.floor(((performance.now() - clock) % model.duration_ms) / model.duration_ms * model.samples) : 0);
    function loop() {
      render();
      timer = opts.animate ? requestAnimationFrame(loop) : null;
    }
    function startLoop() {
      if (moving && opts.animate && timer === null) timer = requestAnimationFrame(loop);
    }

    function draw() {
      if (dirty) return;
      dirty = true;
      requestAnimationFrame(render);
    }

    function render() {
      dirty = false;
      const dpr = window.devicePixelRatio || 1;
      const w = Math.max(1, Math.round(canvas.clientWidth * dpr)), h = Math.max(1, Math.round(canvas.clientHeight * dpr));
      if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
      gl.viewport(0, 0, w, h);
      gl.clearColor(0.1, 0.1, 0.12, 1);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      gl.enable(gl.DEPTH_TEST);
      gl.disable(gl.CULL_FACE);
      const view = mat.lookAt(eyeOf(cam), cam.target, [0, 1, 0]);
      const proj = mat.perspective(0.9, w / h, radius * 0.02, radius * 40);
      gl.useProgram(prog);
      gl.uniformMatrix4fv(U.mvp, false, new Float32Array(mat.mul(proj, view)));
      gl.uniformMatrix3fv(U.nm, false, new Float32Array([view[0], view[1], view[2], view[4], view[5], view[6], view[8], view[9], view[10]]));
      gl.uniform3f(U.light, 0.35, 0.8, 0.5);
      gl.uniform1i(U.lit, opts.lit ? 1 : 0);
      gl.uniform1f(U.alpha, model.alpha ?? 1);
      gl.uniform1i(U.tex, 0);
      const sample = sampleNow();
      for (const node of nodes) {
        gl.uniformMatrix4fv(U.model, false, node.matrices.subarray(sample * 16, sample * 16 + 16));
        for (const p of node.parts) {
          gl.bindVertexArray(p.vao);
          gl.uniform1i(U.has, p.texture ? 1 : 0);
          gl.uniform3f(U.flat, 0.7, 0.7, 0.75);
          if (p.texture) gl.bindTexture(gl.TEXTURE_2D, p.texture);
          if (opts.wire) {
            gl.uniform1i(U.has, 0);
            gl.uniform3f(U.flat, 0.85, 0.85, 0.9);
            for (let i = 0; i + 2 < p.count; i += 3) gl.drawArrays(gl.LINE_LOOP, i, 3);
          } else {
            gl.drawArrays(gl.TRIANGLES, 0, p.count);
          }
        }
      }
    }

    const home = { yaw: 0.6, pitch: 0.35, dist: radius * 2.6, target: [...c] };
    orbit(canvas, cam, home, radius, draw);
    const ro = new ResizeObserver(draw);
    ro.observe(canvas);
    draw();
    startLoop();

    return {
      canvas,
      set(name, value) { opts[name] = value; if (name === "animate") startLoop(); draw(); },
      /** Index of the animation sample shown right now (for tests). */
      sample: sampleNow,
      destroy() { opts.animate = false; if (timer !== null) cancelAnimationFrame(timer); ro.disconnect(); gl.getExtension("WEBGL_lose_context")?.loseContext(); canvas.remove(); },
    };
  }


  // ---------- whole maps: ground, models, water ----------

  const GROUND_VS = `#version 300 es
in vec3 aPos; in vec3 aNormal; in vec2 aUv; in vec2 aLuv;
uniform mat4 uVp;
out vec3 vNormal; out vec2 vUv; out vec2 vLuv;
void main() { vNormal = aNormal; vUv = aUv; vLuv = aLuv; gl_Position = uVp * vec4(aPos, 1.0); }`;
  const GROUND_FS = `#version 300 es
precision mediump float;
in vec3 vNormal; in vec2 vUv; in vec2 vLuv;
uniform sampler2D uTex; uniform sampler2D uLight;
uniform vec3 uAmb; uniform vec3 uDif; uniform vec3 uDir; uniform bool uLm; uniform bool uLit;
out vec4 outColor;
void main() {
  vec4 t = texture(uTex, vUv);
  if (t.a < 0.5) discard;
  float ndl = max(dot(normalize(vNormal), uDir), 0.0);
  vec3 l = uLit ? clamp(uAmb + uDif * ndl, 0.0, 1.0) : vec3(1.0);
  vec4 lm = uLm ? texture(uLight, vLuv) : vec4(1.0, 1.0, 1.0, 1.0);
  vec3 c = t.rgb * l * lm.a;
  outColor = vec4(c, 1.0);
}`;
  // Moving models: aNode picks a row of uMats (one matrix per sample of the loop, 4 texels each),
  // aSpeed (per instance) runs the loop faster or slower; static models skip all of it.
  const MODEL_VS = `#version 300 es
precision highp float; precision highp sampler2D;
in vec3 aPos; in vec3 aNormal; in vec2 aUv; in float aNode; in float aSpeed;
in vec4 aM0; in vec4 aM1; in vec4 aM2; in vec4 aM3;
uniform mat4 uVp; uniform bool uAnim; uniform sampler2D uMats;
uniform float uDuration; uniform float uSamples; uniform float uTime;
out vec3 vNormal; out vec2 vUv;
void main() {
  mat4 m = mat4(aM0, aM1, aM2, aM3);
  vec4 p = vec4(aPos, 1.0); vec3 n = aNormal;
  if (uAnim) {
    float at = mod(uTime * aSpeed, uDuration);
    int s = min(int(at / uDuration * uSamples), int(uSamples) - 1);
    int row = int(aNode + 0.5);
    mat4 a = mat4(texelFetch(uMats, ivec2(s * 4, row), 0), texelFetch(uMats, ivec2(s * 4 + 1, row), 0), texelFetch(uMats, ivec2(s * 4 + 2, row), 0), texelFetch(uMats, ivec2(s * 4 + 3, row), 0));
    p = a * p; n = mat3(a) * n;
  }
  vNormal = mat3(m) * n; vUv = aUv;
  gl_Position = uVp * m * p;
}`;
  const MODEL_FS = `#version 300 es
precision mediump float;
in vec3 vNormal; in vec2 vUv;
uniform sampler2D uTex; uniform bool uHasTex;
uniform vec3 uAmb; uniform vec3 uDif; uniform vec3 uDir; uniform bool uLit;
out vec4 outColor;
void main() {
  vec4 t = uHasTex ? texture(uTex, vUv) : vec4(0.7, 0.7, 0.75, 1.0);
  if (t.a < 0.5) discard;
  float ndl = max(dot(normalize(vNormal), uDir), 0.0);
  vec3 l = uLit ? clamp(uAmb + uDif * ndl, 0.0, 1.0) : vec3(1.0);
  outColor = vec4(t.rgb * l, 1.0);
}`;
  const WATER_VS = `#version 300 es
in vec3 aPos; in vec2 aUv;
uniform mat4 uVp; out vec2 vUv;
void main() { vUv = aUv; gl_Position = uVp * vec4(aPos, 1.0); }`;
  const WATER_FS = `#version 300 es
precision mediump float;
in vec2 vUv; uniform sampler2D uTex; uniform bool uHasTex; out vec4 outColor;
void main() { vec3 c = uHasTex ? texture(uTex, vUv).rgb : vec3(0.2, 0.45, 0.8); outColor = vec4(c * vec3(0.85, 0.95, 1.0), 0.6); }`;

  const b64bytes = (b64) => {
    const bin = atob(b64);
    const u = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) u[i] = bin.charCodeAt(i);
    return u;
  };

  /** Builds the ground mesh, one vertex list per texture: [x y z, nx ny nz, u v, lu lv] per vertex, in viewer axes (y up, z towards the south). */
  function buildGround(g, nTextures) {
    const tilesRaw = b64bytes(g.tiles), cubesRaw = b64bytes(g.cubes);
    const tv = new DataView(tilesRaw.buffer), cv = new DataView(cubesRaw.buffer);
    const nTiles = tilesRaw.length / 36, w = g.width, h = g.height, s = g.zoom / 5;
    const tile = (i) => {
      const o = i * 36;
      const f = (k) => tv.getFloat32(o + k * 4, true);
      return { u: [f(0), f(1), f(2), f(3)], v: [f(4), f(5), f(6), f(7)], tex: tv.getUint16(o + 32, true), light: tv.getUint16(o + 34, true) };
    };
    const cube = (x, y) => {
      const o = (x + y * w) * 28;
      return { h: [0, 1, 2, 3].map((k) => cv.getFloat32(o + k * 4, true) / 5), up: cv.getInt32(o + 16, true), front: cv.getInt32(o + 20, true), right: cv.getInt32(o + 24, true) };
    };
    // lightmap atlas: 8 x 8 per lightmap; rgb colour and a brightness in alpha
    const lw = Math.max(1, Math.round(Math.sqrt(g.lightmap_count))), lh = Math.max(1, Math.ceil(Math.sqrt(g.lightmap_count)));
    const LW = 2 ** Math.ceil(Math.log2(lw * 8)), LH = 2 ** Math.ceil(Math.log2(lh * 8));
    const atlas = new Uint8Array(LW * LH * 4);
    const lm = b64bytes(g.lightmaps);
    for (let i = 0; i < g.lightmap_count; i++) {
      const base = i * 256, ox = (i % lw) * 8, oy = Math.floor(i / lw) * 8;
      for (let y = 0; y < 8; y++) for (let x = 0; x < 8; x++) {
        const d = ((ox + x) + (oy + y) * LW) * 4;
        atlas[d] = lm[base + 64 + (x + y * 8) * 3]; atlas[d + 1] = lm[base + 64 + (x + y * 8) * 3 + 1]; atlas[d + 2] = lm[base + 64 + (x + y * 8) * 3 + 2]; atlas[d + 3] = lm[base + x + y * 8];
      }
    }
    const lightUv = (i) => ({
      u1: (((i % lw) + 0.125) / lw) * ((lw * 8) / LW), u2: (((i % lw) + 0.875) / lw) * ((lw * 8) / LW),
      v1: ((Math.floor(i / lw) + 0.125) / lh) * ((lh * 8) / LH), v2: ((Math.floor(i / lw) + 0.875) / lh) * ((lh * 8) / LH),
    });
    // smooth normals of the top surfaces (average of the four cells around a corner)
    const flat = new Float32Array(w * h * 3);
    const norm = (a, b, c, d) => {
      const u = [c[0] - a[0], c[1] - a[1], c[2] - a[2]], v = [d[0] - b[0], d[1] - b[1], d[2] - b[2]];
      const n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
      const l = Math.hypot(...n) || 1;
      return n.map((q) => q / l);
    };
    const pt = (x, y, hh) => [x * s, -hh, -y * s];
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      const c = cube(x, y);
      if (c.up < 0) continue;
      let n = norm(pt(x, y, c.h[0]), pt(x + 1, y, c.h[1]), pt(x + 1, y + 1, c.h[3]), pt(x, y + 1, c.h[2]));
      if (n[1] < 0) n = n.map((q) => -q);
      flat.set(n, (x + y * w) * 3);
    }
    const cornerNormal = (x, y, dx, dy) => {
      const sum = [0, 0, 0];
      for (const [ox, oy] of [[0, 0], [dx, 0], [dx, dy], [0, dy]]) {
        const cx = x + ox, cy = y + oy;
        if (cx < 0 || cy < 0 || cx >= w || cy >= h) continue;
        for (let k = 0; k < 3; k++) sum[k] += flat[(cx + cy * w) * 3 + k];
      }
      const l = Math.hypot(...sum) || 1;
      return sum.map((q) => q / l);
    };
    const buckets = Array.from({ length: nTextures }, () => []);
    const push = (t, p, n, uv, lu) => buckets[Math.min(t.tex, nTextures - 1)].push(p[0], p[1], p[2], n[0], n[1], n[2], uv[0], uv[1], lu[0], lu[1]);
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      const a = cube(x, y);
      if (a.up >= 0 && a.up < nTiles) {
        const t = tile(a.up), l = lightUv(t.light);
        const P = [pt(x, y, a.h[0]), pt(x + 1, y, a.h[1]), pt(x + 1, y + 1, a.h[3]), pt(x, y + 1, a.h[2])];
        const N = [cornerNormal(x, y, -1, -1), cornerNormal(x, y, 1, -1), cornerNormal(x, y, 1, 1), cornerNormal(x, y, -1, 1)];
        const UV = [[t.u[0], t.v[0]], [t.u[1], t.v[1]], [t.u[3], t.v[3]], [t.u[2], t.v[2]]];
        const LU = [[l.u1, l.v1], [l.u2, l.v1], [l.u2, l.v2], [l.u1, l.v2]];
        for (const i of [0, 1, 2, 2, 3, 0]) push(t, P[i], N[i], UV[i], LU[i]);
      }
      if (a.front >= 0 && a.front < nTiles && y + 1 < h) {
        const t = tile(a.front), b = cube(x, y + 1), l = lightUv(t.light);
        const P = [pt(x, y + 1, b.h[0]), pt(x + 1, y + 1, a.h[3]), pt(x + 1, y + 1, b.h[1]), pt(x, y + 1, b.h[0]), pt(x + 1, y + 1, a.h[3]), pt(x, y + 1, a.h[2])];
        const UV = [[t.u[2], t.v[2]], [t.u[1], t.v[1]], [t.u[3], t.v[3]], [t.u[2], t.v[2]], [t.u[1], t.v[1]], [t.u[0], t.v[0]]];
        const LU = [[l.u1, l.v2], [l.u2, l.v1], [l.u2, l.v2], [l.u1, l.v2], [l.u2, l.v1], [l.u1, l.v1]];
        P.forEach((p, i) => push(t, p, [0, 0, -1], UV[i], LU[i]));
      }
      if (a.right >= 0 && a.right < nTiles && x + 1 < w) {
        const t = tile(a.right), b = cube(x + 1, y), l = lightUv(t.light);
        const P = [pt(x + 1, y, a.h[1]), pt(x + 1, y + 1, a.h[3]), pt(x + 1, y, b.h[0]), pt(x + 1, y, b.h[0]), pt(x + 1, y + 1, b.h[2]), pt(x + 1, y + 1, a.h[3])];
        const UV = [[t.u[1], t.v[1]], [t.u[0], t.v[0]], [t.u[3], t.v[3]], [t.u[3], t.v[3]], [t.u[2], t.v[2]], [t.u[0], t.v[0]]];
        const LU = [[l.u2, l.v1], [l.u1, l.v1], [l.u2, l.v2], [l.u2, l.v2], [l.u1, l.v2], [l.u1, l.v1]];
        P.forEach((p, i) => push(t, p, [1, 0, 0], UV[i], LU[i]));
      }
    }
    return { buckets, atlas, LW, LH, width: w * s, depth: h * s };
  }

  /** Shows a map (`scene` preview): the ground with its lightmaps, the models of the world file and the water. */
  function showScene(host, sc) {
    const canvas = document.createElement("canvas");
    canvas.className = "gl";
    host.append(canvas);
    const gl = canvas.getContext("webgl2", { antialias: true, alpha: false });
    if (!gl) {
      canvas.replaceWith(Object.assign(document.createElement("p"), { className: "error", textContent: "WebGL 2 is not available in this window." }));
      return { destroy() {}, set() {} };
    }
    let dirty = false;
    const draw = () => { if (!dirty) { dirty = true; requestAnimationFrame(render); } };
    const textures = sc.textures.map((t) => makeTexture(gl, t.png, draw));
    const gp = program(gl, GROUND_VS, GROUND_FS), mp = program(gl, MODEL_VS, MODEL_FS), wp = program(gl, WATER_VS, WATER_FS);
    const u = (p, n) => gl.getUniformLocation(p, n);

    // ground
    const ground = buildGround(sc.ground, sc.textures.length);
    const groundParts = [];
    ground.buckets.forEach((verts, ti) => {
      if (!verts.length) return;
      const vao = gl.createVertexArray();
      gl.bindVertexArray(vao);
      const buf = gl.createBuffer();
      gl.bindBuffer(gl.ARRAY_BUFFER, buf);
      gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(verts), gl.STATIC_DRAW);
      [["aPos", 3, 0], ["aNormal", 3, 3], ["aUv", 2, 6], ["aLuv", 2, 8]].forEach(([n, size, off]) => {
        const l = gl.getAttribLocation(gp, n);
        gl.enableVertexAttribArray(l);
        gl.vertexAttribPointer(l, size, gl.FLOAT, false, 40, off * 4);
      });
      groundParts.push({ vao, count: verts.length / 10, tex: textures[ti] });
    });
    const lightTex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, lightTex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, ground.LW, ground.LH, 0, gl.RGBA, gl.UNSIGNED_BYTE, ground.atlas);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);

    // models, drawn instanced
    let animatedModels = 0;
    const perModel = sc.models.map(() => []);
    const perSpeed = sc.models.map(() => []);
    sc.instances.forEach((inst) => { perModel[inst.model].push(...inst.matrix); perSpeed[inst.model].push(inst.speed ?? 1); });
    const modelParts = [];
    sc.models.forEach((m, mi) => {
      if (!perModel[mi].length) return;
      const instBuf = gl.createBuffer();
      gl.bindBuffer(gl.ARRAY_BUFFER, instBuf);
      gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(perModel[mi]), gl.STATIC_DRAW);
      let anim = null;
      let speedBuf = null;
      if (m.anim) {
        const tex = gl.createTexture();
        gl.bindTexture(gl.TEXTURE_2D, tex);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA32F, m.anim.samples * 4, m.anim.node_count, 0, gl.RGBA, gl.FLOAT, f32(m.anim.matrices));
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
        anim = { tex, duration: m.anim.duration_ms, samples: m.anim.samples };
        speedBuf = gl.createBuffer();
        gl.bindBuffer(gl.ARRAY_BUFFER, speedBuf);
        gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(perSpeed[mi]), gl.STATIC_DRAW);
        animatedModels++;
      }
      for (const p of m.parts) {
        const vao = gl.createVertexArray();
        gl.bindVertexArray(vao);
        const pos = f32(p.positions);
        [["aPos", pos, 3], ["aNormal", f32(p.normals), 3], ["aUv", f32(p.uvs), 2]].forEach(([n, data, size]) => {
          const b = gl.createBuffer();
          gl.bindBuffer(gl.ARRAY_BUFFER, b);
          gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
          const l = gl.getAttribLocation(mp, n);
          gl.enableVertexAttribArray(l);
          gl.vertexAttribPointer(l, size, gl.FLOAT, false, 0, 0);
        });
        if (anim) {
          const nb = gl.createBuffer();
          gl.bindBuffer(gl.ARRAY_BUFFER, nb);
          gl.bufferData(gl.ARRAY_BUFFER, f32(p.nodes), gl.STATIC_DRAW);
          const ln = gl.getAttribLocation(mp, "aNode");
          gl.enableVertexAttribArray(ln);
          gl.vertexAttribPointer(ln, 1, gl.FLOAT, false, 0, 0);
          gl.bindBuffer(gl.ARRAY_BUFFER, speedBuf);
          const ls = gl.getAttribLocation(mp, "aSpeed");
          gl.enableVertexAttribArray(ls);
          gl.vertexAttribPointer(ls, 1, gl.FLOAT, false, 0, 0);
          gl.vertexAttribDivisor(ls, 1);
        }
        gl.bindBuffer(gl.ARRAY_BUFFER, instBuf);
        for (let k = 0; k < 4; k++) {
          const l = gl.getAttribLocation(mp, `aM${k}`);
          gl.enableVertexAttribArray(l);
          gl.vertexAttribPointer(l, 4, gl.FLOAT, false, 64, k * 16);
          gl.vertexAttribDivisor(l, 1);
        }
        modelParts.push({ vao, count: pos.length / 3, instances: perModel[mi].length / 16, tex: textures[p.texture], anim });
      }
    });

    // water: one plane over the ground
    let water = null;
    if (sc.water) {
      const W = ground.width, D = ground.depth, y = -sc.water.level, t = 0.2;
      const quad = new Float32Array([0, y, 0, 0, 0, W, y, 0, W * t, 0, W, y, -D, W * t, D * t, 0, y, 0, 0, 0, W, y, -D, W * t, D * t, 0, y, -D, 0, D * t]);
      const vao = gl.createVertexArray();
      gl.bindVertexArray(vao);
      const b = gl.createBuffer();
      gl.bindBuffer(gl.ARRAY_BUFFER, b);
      gl.bufferData(gl.ARRAY_BUFFER, quad, gl.STATIC_DRAW);
      [["aPos", 3, 0], ["aUv", 2, 3]].forEach(([n, size, off]) => {
        const l = gl.getAttribLocation(wp, n);
        gl.enableVertexAttribArray(l);
        gl.vertexAttribPointer(l, size, gl.FLOAT, false, 20, off * 4);
      });
      water = { vao, frameMs: sc.water.frame_ms || 50, frames: sc.water.frames.map((f) => makeTexture(gl, f, draw, "image/jpeg")) };
    }

    const lon = (sc.light.longitude * Math.PI) / 180, lat = (sc.light.latitude * Math.PI) / 180;
    const dir = (() => { const d = [-Math.cos(lon) * Math.sin(lat), Math.cos(lat), Math.sin(lon) * Math.sin(lat)]; const l = Math.hypot(...d) || 1; return d.map((q) => q / l); })();
    const amb = sc.light.ambient.map((q) => q * sc.light.intensity), dif = sc.light.diffuse;
    const size = Math.max(ground.width, ground.depth);
    const home = { yaw: 0, pitch: 0.95, dist: size * 0.9, target: [ground.width / 2, 0, -ground.depth / 2] };
    const cam = { ...home, target: [...home.target] };
    const opts = { lit: true, lightmap: true, models: true, water: true, animate: true };
    const waterFrame = () => (water && opts.animate && water.frames.length > 1 ? Math.floor((performance.now() - clock) / water.frameMs) % water.frames.length : 0);
    const clock = performance.now();
    let timer = null;
    const moving = () => opts.animate && ((water && opts.water && water.frames.length > 1) || (opts.models && animatedModels > 0));
    function loop() {
      render();
      timer = moving() ? requestAnimationFrame(loop) : null;
    }
    function startLoop() {
      if (timer === null && moving()) timer = requestAnimationFrame(loop);
    }
    orbit(canvas, cam, home, size, draw);

    function render() {
      dirty = false;
      const dpr = window.devicePixelRatio || 1;
      const w = Math.max(1, Math.round(canvas.clientWidth * dpr)), h = Math.max(1, Math.round(canvas.clientHeight * dpr));
      if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
      gl.viewport(0, 0, w, h);
      gl.clearColor(0.45, 0.6, 0.8, 1);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      gl.enable(gl.DEPTH_TEST);
      gl.disable(gl.CULL_FACE);
      gl.disable(gl.BLEND);
      const vp = new Float32Array(mat.mul(mat.perspective(0.8, w / h, size * 0.002, size * 12), mat.lookAt(eyeOf(cam), cam.target, [0, 1, 0])));
      gl.useProgram(gp);
      gl.uniformMatrix4fv(u(gp, "uVp"), false, vp);
      gl.uniform3fv(u(gp, "uAmb"), amb); gl.uniform3fv(u(gp, "uDif"), dif); gl.uniform3fv(u(gp, "uDir"), dir);
      gl.uniform1i(u(gp, "uLit"), opts.lit ? 1 : 0); gl.uniform1i(u(gp, "uLm"), opts.lightmap ? 1 : 0);
      gl.uniform1i(u(gp, "uTex"), 0); gl.uniform1i(u(gp, "uLight"), 1);
      gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D, lightTex);
      gl.activeTexture(gl.TEXTURE0);
      for (const p of groundParts) { gl.bindVertexArray(p.vao); gl.bindTexture(gl.TEXTURE_2D, p.tex); gl.drawArrays(gl.TRIANGLES, 0, p.count); }
      if (opts.models) {
        gl.useProgram(mp);
        gl.uniformMatrix4fv(u(mp, "uVp"), false, vp);
        gl.uniform3fv(u(mp, "uAmb"), amb); gl.uniform3fv(u(mp, "uDif"), dif); gl.uniform3fv(u(mp, "uDir"), dir);
        gl.uniform1i(u(mp, "uLit"), opts.lit ? 1 : 0); gl.uniform1i(u(mp, "uTex"), 0);
        gl.uniform1i(u(mp, "uMats"), 2);
        gl.uniform1f(u(mp, "uTime"), opts.animate ? performance.now() - clock : 0);
        for (const p of modelParts) {
          gl.bindVertexArray(p.vao);
          gl.uniform1i(u(mp, "uAnim"), p.anim ? 1 : 0);
          if (p.anim) {
            gl.uniform1f(u(mp, "uDuration"), p.anim.duration); gl.uniform1f(u(mp, "uSamples"), p.anim.samples);
            gl.activeTexture(gl.TEXTURE2); gl.bindTexture(gl.TEXTURE_2D, p.anim.tex); gl.activeTexture(gl.TEXTURE0);
          }
          gl.uniform1i(u(mp, "uHasTex"), p.tex ? 1 : 0);
          if (p.tex) gl.bindTexture(gl.TEXTURE_2D, p.tex);
          gl.drawArraysInstanced(gl.TRIANGLES, 0, p.count, p.instances);
        }
      }
      if (water && opts.water) {
        gl.useProgram(wp);
        gl.uniformMatrix4fv(u(wp, "uVp"), false, vp);
        const wtex = water.frames[waterFrame()];
        gl.uniform1i(u(wp, "uTex"), 0); gl.uniform1i(u(wp, "uHasTex"), wtex ? 1 : 0);
        gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA); gl.depthMask(false);
        gl.bindVertexArray(water.vao);
        if (wtex) gl.bindTexture(gl.TEXTURE_2D, wtex);
        gl.drawArrays(gl.TRIANGLES, 0, 6);
        gl.depthMask(true); gl.disable(gl.BLEND);
      }
    }

    const ro = new ResizeObserver(draw);
    ro.observe(canvas);
    draw();
    startLoop();
    return {
      canvas,
      set(name, value) { opts[name] = value; startLoop(); draw(); },
      /** Index of the water picture shown right now (for tests). */
      waterFrame,
      animatedModels: () => animatedModels,
      destroy() { opts.animate = false; if (timer !== null) cancelAnimationFrame(timer); ro.disconnect(); gl.getExtension("WEBGL_lose_context")?.loseContext(); canvas.remove(); },
    };
  }

  return { show, showScene };
})();
