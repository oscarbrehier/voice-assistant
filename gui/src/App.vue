<script setup lang="ts">
import { listen } from "@tauri-apps/api/event";
import SpeechBlob from "./components/SpeechBlob.vue";
import { nextTick, onMounted, ref } from "vue";
import { Ellipsis, PinOffIcon } from "@lucide/vue";
import { getCurrentWindow, LogicalPosition, LogicalSize } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";

const STATES = {
	IDLE: "idle",
	RECORDING: "recording",
	ACTIVE: "active",
	PROCESSING: "processing",
	SPEAKING: "speaking"
} as const;

type State = typeof STATES[keyof typeof STATES];

type VolumePacket = { type: "Volume"; content: number };
type TranscriptionPacket = { type: "Transcription"; content: string };

type Packet = VolumePacket | TranscriptionPacket;

type UIEvent = {
	state: State,
	data: Packet
};

let audioLevel = ref(0);
let state = ref<State>("idle");
let transcription = ref<string | null>(null);

listen<UIEvent>("engine-update", ({ payload }) => {

	state.value = payload.state;
	console.log(payload.state)

	const shouldExpand = payload.state != "idle";

	if (shouldExpand) {
		toggleExpand(true);
	} else {
		toggleExpand(false);
	};

	switch (payload.data.type) {
		case "Volume":
			audioLevel.value = payload.data.content;
			break;

		case "Transcription":
			transcription.value = payload.data.content;

			break;
	};


});

const BASE_HEIGHT = 80;
const BASE_WIDTH = 200;
const EXPANDED_WIDTH = 480;

const isReady = ref(false);
const isExpanded = ref(false);

const appWindow = getCurrentWindow();

async function toggleExpand(overrideState?: boolean) {

	if (!isReady.value) return;

	let expand = overrideState ?? !isExpanded.value;
	if (expand === isExpanded.value) return;
	
	isExpanded.value = expand;

	const targetWidth = expand ? EXPANDED_WIDTH : BASE_WIDTH;

	if (expand) {
		await invoke("set_window_size", { width: targetWidth, height: BASE_HEIGHT });
	} else {
		setTimeout(async () => {

			if (!isExpanded.value) {
				await invoke("set_window_size", { width: targetWidth, height: BASE_HEIGHT });
			};

		}, 500);

	};

};

onMounted(() => {
	appWindow.setResizable(false);

	setTimeout(async () => {
		isReady.value = true;
	}, 500);
});

</script>

<template>
	<main class="w-full h-full flex justify-end">
		<section
			class="h-screen bg-neutral-900 rounded-[3rem] flex justify-between items-center border border-neutral-600 pr-4 relative overflow-hidden z-10"
			:class="[
				'transition-all duration-500 ease-in-out',
			]" :style="{ width: (isExpanded ? EXPANDED_WIDTH : BASE_WIDTH) + 'px' }" data-tauri-drag-region>

			<section
				class="h-full aspect-square flex items-start justify-start shrink-0 pointer-events-none overflow-hidden">

				<SpeechBlob :audio-level="audioLevel" class="pointer-events-none" />

			</section>

			<!-- <div v-if="isExpanded" class="flex-1 h-auto">
				<p class="text-neutral-100">
					<span class="text-neutral-400">You:</span>
					{{ transcription }}
				</p>
			</div> -->

			<div class="flex space-x-2">
				<button @click="toggleExpand()" class="rounded-full text-neutral-300 bg-neutral-900 p-2.5">
					<PinOffIcon :size="15" />
				</button>

				<button @click="toggleExpand()" class="rounded-full text-neutral-300  bg-neutral-900 p-2.5">
					<Ellipsis :size="18" />
				</button>
			</div>

		</section>
	</main>
</template>

<style>
html,
body {
	background: transparent !important;
}

.deep-purple-island {
	background-color: #0F0A1F;

	background-image:
		radial-gradient(at 40% 0%, rgba(183, 64, 220, 0.20) 0%, transparent 70%),
		radial-gradient(at 50% 100%, rgba(32, 7, 114, 0.40) 0%, transparent 50%);

	background-blend-mode: screen;
	box-shadow: inset 0 0.5px 1px rgba(255, 255, 255, 0.15);
}
</style>