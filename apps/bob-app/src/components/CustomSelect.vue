<script setup>
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue';

const props = defineProps({
  modelValue: {
    type: [String, Number],
    default: '',
  },
  options: {
    type: Array,
    default: () => [],
  },
  disabled: {
    type: Boolean,
    default: false,
  },
  ariaLabel: {
    type: String,
    default: '',
  },
});

const emit = defineEmits(['update:modelValue', 'change']);

const isOpen = ref(false);
const wrapperRef = ref(null);
const menuRef = ref(null);
const insideDialog = ref(false);
const dropdownStyle = ref({});

const selectedOption = computed(() =>
  (props.options || []).find(opt => opt.value === props.modelValue),
);

const selectedLabel = computed(() =>
  selectedOption.value ? selectedOption.value.label : String(props.modelValue || ''),
);

function updatePosition() {
  if (!wrapperRef.value) return;
  const rect = wrapperRef.value.getBoundingClientRect();
  const viewportHeight = window.innerHeight || document.documentElement.clientHeight || 800;
  const viewportWidth = window.innerWidth || document.documentElement.clientWidth || 390;
  const maxMenuHeight = 240;
  const spaceBelow = viewportHeight - rect.bottom - 8;
  const openUpward = spaceBelow < 140 && rect.top > spaceBelow;
  const width = Math.max(rect.width, 136);
  const left = Math.min(Math.max(8, rect.left), Math.max(8, viewportWidth - width - 8));

  if (openUpward) {
    dropdownStyle.value = {
      position: 'fixed',
      bottom: `${Math.max(8, viewportHeight - rect.top + 4)}px`,
      left: `${left}px`,
      width: `${width}px`,
      maxHeight: `${Math.min(maxMenuHeight, rect.top - 12)}px`,
      zIndex: 9999,
    };
  } else {
    dropdownStyle.value = {
      position: 'fixed',
      top: `${rect.bottom + 4}px`,
      left: `${left}px`,
      width: `${width}px`,
      maxHeight: `${Math.min(maxMenuHeight, Math.max(120, spaceBelow))}px`,
      zIndex: 9999,
    };
  }
}

function toggle() {
  if (props.disabled) return;
  insideDialog.value = !!wrapperRef.value?.closest("dialog");
  isOpen.value = !isOpen.value;
  if (isOpen.value) {
    updatePosition();
    nextTick(updatePosition);
  }
}

function selectOption(option) {
  if (!option || option.disabled) return;
  emit('update:modelValue', option.value);
  emit('change', option.value);
  isOpen.value = false;
}

function onTriggerKeydown(event) {
  if (props.disabled) return;
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault();
    toggle();
  } else if (event.key === 'Escape' && isOpen.value) {
    event.preventDefault();
    event.stopPropagation();
    isOpen.value = false;
  } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    const enabled = (props.options || []).filter(o => !o.disabled);
    if (!enabled.length) return;
    const idx = enabled.findIndex(o => o.value === props.modelValue);
    const delta = event.key === 'ArrowDown' ? 1 : -1;
    const nextIdx = idx < 0 ? 0 : (idx + delta + enabled.length) % enabled.length;
    selectOption(enabled[nextIdx]);
  }
}

function handleClickOutside(event) {
  if (!isOpen.value) return;
  if (wrapperRef.value && wrapperRef.value.contains(event.target)) return;
  if (menuRef.value && menuRef.value.contains(event.target)) return;
  isOpen.value = false;
}

function handleScrollOrResize() {
  if (isOpen.value) {
    updatePosition();
  }
}

onMounted(() => {
  document.addEventListener('mousedown', handleClickOutside);
  document.addEventListener('scroll', handleScrollOrResize, true);
  window.addEventListener('resize', handleScrollOrResize);
});

onUnmounted(() => {
  document.removeEventListener('mousedown', handleClickOutside);
  document.removeEventListener('scroll', handleScrollOrResize, true);
  window.removeEventListener('resize', handleScrollOrResize);
});
</script>

<template>
  <div
    ref="wrapperRef"
    class="custom-select-wrapper"
    :class="{ open: isOpen, disabled }"
    :tabindex="disabled ? -1 : 0"
    role="combobox"
    :aria-label="ariaLabel || undefined"
    :aria-expanded="isOpen"
    aria-haspopup="listbox"
    @click.stop="toggle"
    @keydown="onTriggerKeydown"
  >
    <div class="custom-select-trigger" :class="{ 'is-open': isOpen }">
      <span class="custom-select-label">
        <slot name="selected" :option="selectedOption" :label="selectedLabel">
          {{ selectedLabel }}
        </slot>
      </span>
      <svg
        class="chevron-svg custom-select-chevron"
        :class="{ 'rotate-180': isOpen }"
        viewBox="0 0 24 24"
        aria-hidden="true"
      >
        <path d="m6 9 6 6 6-6" />
      </svg>
    </div>

    <Teleport to="body" :disabled="insideDialog">
      <ul
        v-if="isOpen"
        ref="menuRef"
        class="custom-select-options"
        role="listbox"
        :style="dropdownStyle"
      >
        <li
          v-for="option in options"
          :key="option.value"
          role="option"
          :aria-selected="option.value === modelValue"
          class="custom-select-option"
          :class="{
            selected: option.value === modelValue,
            'is-selected': option.value === modelValue,
            'is-disabled': option.disabled,
          }"
          @click.stop="!option.disabled && selectOption(option)"
        >
          <slot name="option" :option="option" :label="option.label">
            <span class="custom-select-option-text">{{ option.label }}</span>
          </slot>
          <svg
            v-if="option.value === modelValue"
            class="custom-select-check"
            viewBox="0 0 24 24"
            aria-hidden="true"
          >
            <path d="M20 6 9 17l-5-5" />
          </svg>
        </li>
      </ul>
    </Teleport>
  </div>
</template>
