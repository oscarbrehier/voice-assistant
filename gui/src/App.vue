<script setup lang="ts">
import { listen } from "@tauri-apps/api/event";
import SpeechBlob from "./components/SpeechBlob.vue";
import { ref } from "vue";
import { Settings } from "@lucide/vue";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

type State = "idle" | "recording" | "active";

type VolumePacket = { type: "Volume"; payload: number };
type TranscriptionPacket = { type: "Transcription"; payload: string };

type Packet = VolumePacket | TranscriptionPacket;

type UIEvent = {
	state: State,
	data: Packet
};

let audioLevel = ref(0);
let state = ref<State>("idle");

listen<UIEvent>("engine-update", ({ payload }) => {

	console.log("payload")

	state.value = payload.state;

	console.log(payload.state)

	if (payload.state == "active" || payload.state == "recording") {
		toggleExpand(true);
	} else {
		toggleExpand(false);
	};

	if (payload.data.type == "Volume") {
		audioLevel.value = payload.data.payload;
	};
});

const BASE_HEIGHT = 80;
const BASE_WIDTH = 140;
const EXPANDED_WIDTH = 480;

const isExpanded = ref(false);

async function toggleExpand(overrideState?: boolean) {
	let expand = overrideState ?? !isExpanded.value;

	isExpanded.value = expand;

	if (expand) {
		getCurrentWindow().setSize(new LogicalSize(EXPANDED_WIDTH, BASE_HEIGHT));
	} else {
		setTimeout(() => {
			if (!isExpanded.value) {
				getCurrentWindow().setSize(new LogicalSize(BASE_WIDTH, BASE_HEIGHT));
			};
		}, 500);
	};
};

</script>

<template>
	<main class="w-full h-full flex justify-end">
		<section
			class="h-screen bg-neutral-900 rounded-[3rem] flex justify-between items-center border border-neutral-700 pr-4"
			:class="[
				'transition-all duration-500 ease-in-out',
			]" :style="{ width: (isExpanded ? EXPANDED_WIDTH : BASE_WIDTH) + 'px' }" data-tauri-drag-region>

			<section
				class="h-full aspect-square flex items-start justify-start shrink-0 pointer-events-none overflow-hidden">

				<SpeechBlob :audio-level="audioLevel" class="pointer-events-none" />

			</section>

			<button @click="toggleExpand()" class="p-2 rounded-full text-neutral-300">
				<Settings :size="18" />
			</button>

		</section>
	</main>
</template>

<style>
html,
body {
	background: transparent !important;
}
</style>