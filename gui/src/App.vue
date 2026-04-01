<script setup lang="ts">
import { listen } from "@tauri-apps/api/event";
import SpeechBlob from "./components/SpeechBlob.vue";
import { ref } from "vue";

let audioLevel = ref(0);

listen<number>("audio", (event) => {

	audioLevel.value = event.payload;
})

</script>

<template>
	<main class="h-screen bg-black">

		<SpeechBlob :audio-level="audioLevel" />

	</main>
</template>