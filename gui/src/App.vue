<script setup lang="ts">
import { listen } from "@tauri-apps/api/event";
import SpeechBlob from "./components/SpeechBlob.vue";
import { ref } from "vue";
import { Settings } from "@lucide/vue";

let audioLevel = ref(0);

listen<number>("audio", (event) => {

	audioLevel.value = event.payload;
});

const BASE_WIDTH = 240;
const EXPANDED_WIDTH = 480;

const isExpanded = ref(false);

function toggleExpand() {
	isExpanded.value = !isExpanded.value;
}

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

			<button class="p-2 rounded-full text-neutral-300">
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