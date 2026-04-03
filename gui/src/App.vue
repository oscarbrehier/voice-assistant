<script setup lang="ts">
import { listen } from "@tauri-apps/api/event";
import SpeechBlob from "./components/SpeechBlob.vue";
import { onMounted, ref } from "vue";
import { Ellipsis, PinOffIcon } from "@lucide/vue";
import { getCurrentWindow, LogicalPosition, LogicalSize } from "@tauri-apps/api/window";

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
const BASE_WIDTH = 200;
const EXPANDED_WIDTH = 480;

const isReady = ref(false);
const isExpanded = ref(false);

const appWindow = getCurrentWindow();

async function toggleExpand(overrideState?: boolean) {

	if (!isReady.value) return;

	let expand = overrideState ?? !isExpanded.value;
	if (expand === isExpanded.value) return;


	const [factor, physicalPos, physicalSize] = await Promise.all([
		appWindow.scaleFactor(),
		appWindow.outerPosition(),
		appWindow.outerSize()
	]);

	const logicalPos = physicalPos.toLogical(factor);
	const logicalSize = physicalSize.toLogical(factor);

	const rightEdge = logicalPos.x + logicalSize.width;

	isExpanded.value = expand;

	if (expand) {

		const newX = rightEdge - EXPANDED_WIDTH;

		appWindow.setPosition(new LogicalPosition(newX, logicalPos.y));
		appWindow.setSize(new LogicalSize(EXPANDED_WIDTH, BASE_HEIGHT));

	} else {

		setTimeout(async () => {

			if (!isExpanded.value) {

				const newX = rightEdge - BASE_WIDTH;

				appWindow.setPosition(new LogicalPosition(newX, logicalPos.y));
				appWindow.setSize(new LogicalSize(BASE_WIDTH, BASE_HEIGHT));

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
			class="h-screen bg-neutral-900 rounded-[3rem] flex justify-between items-center border border-neutral-800 pr-4 relative overflow-hidden z-10"
			:class="[
				'transition-all duration-500 ease-in-out',
			]" :style="{ width: (isExpanded ? EXPANDED_WIDTH : BASE_WIDTH) + 'px' }" data-tauri-drag-region>

			<section
				class="h-full aspect-square flex items-start justify-start shrink-0 pointer-events-none overflow-hidden">

				<SpeechBlob :audio-level="audioLevel" class="pointer-events-none" />

			</section>

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