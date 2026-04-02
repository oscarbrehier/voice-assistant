<template>
	<div class="flex items-center justify-center h-full w-full overflow-hidden">
		<canvas ref="canvasRef"
			class="max-h-full max-w-full aspect-square object-contain transition-opacity duration-500"
			:class="isLoaded ? 'opacity-100' : 'opacity-0'"></canvas>
	</div>
</template>

<script setup lang="ts">

/*
	* Original shader: Siri [327] by Xor
	* Source: https://www.shadertoy.com/view/tXGXDK#
*/

/*
	* vec4(0,0,0,0)
	* vec4(4,2,0,0)
	* vec4(0,1,8,0)
	* vec4(1,4,1,0)
*/

import { ref, onMounted, onUnmounted } from 'vue';

const props = defineProps<{
	audioLevel: number
}>();

const canvasRef = ref<HTMLCanvasElement | null>(null);
const isLoaded = ref<boolean>(false);

const smoothedAudio = ref<number>(props.audioLevel);

let animationFrameId: number | null = null;
let gl: WebGL2RenderingContext | null = null;
let program: WebGLProgram | null = null;

const vsSource = `#version 300 es
	in vec2 pos;
	void main() { gl_Position = vec4(pos, 0, 1); }
`;

const fsSource = `#version 300 es
	precision highp float;
	uniform vec3 iResolution;
	uniform float iTime;
	uniform float iAudio;
	out vec4 fragColor;

	float hash(vec2 p) {
		return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
	}

	void main() {
		vec4 finalCol = vec4(0.0);
		
		float padding = 1.9 - (iAudio * 0.5); 
		const vec4 colors = vec4(1,1,2,10);

		for(float m=0.; m<2.; m++) {
			for(float n=0.; n<2.; n++) {
				vec2 offset = vec2(m, n) * 0.5;
				vec2 uv = (2.0 * (gl_FragCoord.xy + offset) - iResolution.xy) / iResolution.y;
				
				uv *= padding; 

				vec4 O = vec4(0.0);
				vec3 p, a;
				float z=0., d=0., s=0., i=0.;
				
				for(i=0.; i<120.; i++) {
					p = z * normalize(vec3(uv, -1.0)); 
					p.z += 9.0;
					
					s = length(p = dot(a = normalize(cos(vec3(0,2,4) - iTime*.5 + s*(0.3 + iAudio*0.2))), p) * a - cross(a, p));
					z += d = min(abs(dot(p, sin(p).yzx)) * .2 + max(d = s - 5., .1), abs(--d) + .2) * .2;
					O += max(cos(p.x * .6 + colors), 5. / s / s) / d / d;
				}
				finalCol += tanh(O / 3e4);
			}
		}
		
		finalCol /= 4.0;
		finalCol.rgb += (hash(gl_FragCoord.xy) - 0.5) * (1.0 / 255.0);
		
		float alpha = clamp(length(finalCol.rgb) * (2.2 + iAudio * 0.5), 0.0, 1.0);
    	fragColor = vec4(finalCol.rgb, alpha);
	}
`;

const createShader = (type: number, source: string): WebGLShader | null => {

	if (!gl) return null;

	const shader = gl.createShader(type);
	if (!shader) return null;

	gl.shaderSource(shader, source);
	gl.compileShader(shader);

	return shader;

};

const initGL = (): void => {

	const canvas = canvasRef.value;
	if (!canvas) return;

	gl = canvas.getContext('webgl2', { antialias: true, alpha: true });
	if (!gl) return;

	const vs = createShader(gl.VERTEX_SHADER, vsSource);
	const fs = createShader(gl.FRAGMENT_SHADER, fsSource);
	if (!vs || !fs) return;

	program = gl.createProgram();
	if (!program) return;

	gl.attachShader(program, vs);
	gl.attachShader(program, fs);
	gl.linkProgram(program);

	const buf = gl.createBuffer();

	gl.bindBuffer(gl.ARRAY_BUFFER, buf);
	gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, -1, 1, 1, -1, 1, 1]), gl.STATIC_DRAW);

	const loc = gl.getAttribLocation(program, "pos");

	gl.enableVertexAttribArray(loc);
	gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);

};

const render = (t: number): void => {

	if (!gl || !program || !canvasRef.value) return;

	const dpr = window.devicePixelRatio || 1;
	const targetW = Math.floor(canvasRef.value.clientWidth * dpr);
	const targetH = Math.floor(canvasRef.value.clientHeight * dpr);

	if (canvasRef.value.width !== targetW || canvasRef.value.height !== targetH) {
		canvasRef.value.width = targetW;
		canvasRef.value.height = targetH;
		gl.viewport(0, 0, canvasRef.value.width, canvasRef.value.height);
	}

	gl.clearColor(0, 0, 0, 0);
	gl.clear(gl.COLOR_BUFFER_BIT);

	gl.useProgram(program);

	const MAX_SCALE = 1.0;

	let target = props.audioLevel;
	if (target > MAX_SCALE) target = MAX_SCALE;

	smoothedAudio.value += (target - smoothedAudio.value) * 0.1;

	console.log(smoothedAudio.value)

	gl.uniform1f(gl.getUniformLocation(program, "iAudio"), smoothedAudio.value);

	gl.uniform3f(gl.getUniformLocation(program, "iResolution"), canvasRef.value.width, canvasRef.value.height, 1);
	gl.uniform1f(gl.getUniformLocation(program, "iTime"), t * 0.001);
	gl.drawArrays(gl.TRIANGLES, 0, 6);


	if (!isLoaded.value) {
		isLoaded.value = true;
	}

	animationFrameId = requestAnimationFrame(render);

};

onMounted(() => {
	initGL();
	animationFrameId = requestAnimationFrame(render);
});

onUnmounted(() => {
	if (animationFrameId) cancelAnimationFrame(animationFrameId);
});

</script>