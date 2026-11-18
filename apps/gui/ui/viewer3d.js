"use strict";

// A small WebGL2 viewer for textured triangle meshes: orbit with the mouse, zoom with the wheel,
// double-click resets. No library: the backend sends positions, normals and uvs as base64 floats.

const Viewer3D = (() => {
  const VS = `#version 300 es
in vec3 aPos; in vec3 aNormal; in vec2 aUv;
uniform mat4 uMvp; uniform mat3 uNormalMat;
out vec3 vNormal; out vec2 vUv;
void main() { vNormal = uNormalMat * aNormal; vUv = aUv; gl_Position = uMvp * vec4(aPos, 1.0); }`;
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
    const U = { mvp: loc("uMvp"), nm: loc("uNormalMat"), tex: loc("uTex"), has: loc("uHasTex"), lit: loc("uLit"), light: loc("uLight"), alpha: loc("uAlpha"), flat: loc("uFlat") };

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

    const parts = model.parts.map((p) => {
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
    });

    const c = [0, 1, 2].map((i) => (model.min[i] + model.max[i]) / 2);
    const radius = Math.max(1, Math.hypot(model.max[0] - model.min[0], model.max[1] - model.min[1], model.max[2] - model.min[2]) / 2);
    const cam = { yaw: 0.6, pitch: 0.35, dist: radius * 2.6 };
    const opts = { lit: true, wire: false };
    let dirty = false;

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
      const eye = [c[0] + cam.dist * Math.cos(cam.pitch) * Math.sin(cam.yaw), c[1] + cam.dist * Math.sin(cam.pitch), c[2] + cam.dist * Math.cos(cam.pitch) * Math.cos(cam.yaw)];
      const view = mat.lookAt(eye, c, [0, 1, 0]);
      const proj = mat.perspective(0.9, w / h, radius * 0.02, radius * 40);
      gl.useProgram(prog);
      gl.uniformMatrix4fv(U.mvp, false, new Float32Array(mat.mul(proj, view)));
      gl.uniformMatrix3fv(U.nm, false, new Float32Array([view[0], view[1], view[2], view[4], view[5], view[6], view[8], view[9], view[10]]));
      gl.uniform3f(U.light, 0.35, 0.8, 0.5);
      gl.uniform1i(U.lit, opts.lit ? 1 : 0);
      gl.uniform1f(U.alpha, model.alpha ?? 1);
      gl.uniform1i(U.tex, 0);
      for (const p of parts) {
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

    let drag = null;
    canvas.addEventListener("pointerdown", (e) => { drag = { x: e.clientX, y: e.clientY }; canvas.setPointerCapture(e.pointerId); });
    canvas.addEventListener("pointerup", () => (drag = null));
    canvas.addEventListener("pointermove", (e) => {
      if (!drag) return;
      cam.yaw -= (e.clientX - drag.x) * 0.01;
      cam.pitch = Math.max(-1.5, Math.min(1.5, cam.pitch + (e.clientY - drag.y) * 0.01));
      drag = { x: e.clientX, y: e.clientY };
      draw();
    });
    canvas.addEventListener("wheel", (e) => { e.preventDefault(); cam.dist = Math.max(radius * 0.3, Math.min(radius * 20, cam.dist * Math.exp(e.deltaY * 0.001))); draw(); }, { passive: false });
    canvas.addEventListener("dblclick", () => { Object.assign(cam, { yaw: 0.6, pitch: 0.35, dist: radius * 2.6 }); draw(); });
    const ro = new ResizeObserver(draw);
    ro.observe(canvas);
    draw();

    return {
      canvas,
      set(name, value) { opts[name] = value; draw(); },
      destroy() { ro.disconnect(); gl.getExtension("WEBGL_lose_context")?.loseContext(); canvas.remove(); },
    };
  }

  return { show };
})();
