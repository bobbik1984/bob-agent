<script setup>
import { ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { kernelBridge } from '../native-bridge.mjs';
const props = defineProps({ captureId: String, mediaId: String });
const { t } = useI18n();
const image = ref(null);
const error = ref(false);
watch(() => [props.mediaId, props.captureId], async ([mId, cId]) => {
  image.value = null; error.value = false;
  if (!mId && !cId) return;
  try {
    const found = await kernelBridge.captureMediaGet(mId || null, cId || null);
    if (mId === props.mediaId && cId === props.captureId) image.value = found;
  } catch {
    error.value = true;
  }
}, { immediate: true });
</script>
<template>
  <figure v-if="image" class="capture-photo-panel"><img :src="`data:${image.mime};base64,${image.base64}`" :alt="t('captureOriginal')" /><figcaption>{{ t('captureImageStored') }}</figcaption></figure>
  <p v-else-if="error" role="status">{{ t('captureImageMissing') }}</p>
</template>
