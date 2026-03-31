<script setup lang="ts">
import { listen } from "@tauri-apps/api/event";
import SpeechBlob from "./components/SpeechBlob.vue";

listen("audio", (event) => {
	console.log(event);
})

</script>

<template>
	<main class="h-screen bg-black">

		<SpeechBlob />

	</main>
</template>