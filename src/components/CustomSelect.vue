<template>
  <div class="custom-select-wrapper" ref="wrapperRef">
    <div
      class="custom-select-trigger input"
      :class="{ 'is-open': isOpen }"
      @click="toggle"
    >
      <span>{{ selectedLabel }}</span>
      <ChevronDown :size="16" class="dropdown-icon" :class="{ 'rotate-180': isOpen }" />
    </div>

    <Transition name="dropdown-fade">
      <ul v-if="isOpen" class="custom-select-options">
        <li
          v-for="option in options"
          :key="option.value"
          class="custom-select-option"
          :class="{ 'is-selected': option.value === modelValue }"
          @click="selectOption(option)"
        >
          {{ option.label }}
        </li>
      </ul>
    </Transition>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { ChevronDown } from 'lucide-vue-next';

const props = defineProps({
  modelValue: {
    type: [String, Number],
    required: true
  },
  options: {
    type: Array,
    required: true,
    // [{ label: 'Option 1', value: '1' }]
  }
});

const emit = defineEmits(['update:modelValue', 'change']);

const isOpen = ref(false);
const wrapperRef = ref(null);

const selectedLabel = computed(() => {
  const selected = props.options.find(opt => opt.value === props.modelValue);
  return selected ? selected.label : '请选择';
});

function toggle() {
  isOpen.value = !isOpen.value;
}

function selectOption(option) {
  emit('update:modelValue', option.value);
  emit('change', option.value);
  isOpen.value = false;
}

function handleClickOutside(event) {
  if (wrapperRef.value && !wrapperRef.value.contains(event.target)) {
    isOpen.value = false;
  }
}

onMounted(() => {
  document.addEventListener('mousedown', handleClickOutside);
});

onUnmounted(() => {
  document.removeEventListener('mousedown', handleClickOutside);
});
</script>

<style scoped>
.custom-select-wrapper {
  position: relative;
  width: 100%;
}

.custom-select-trigger {
  display: flex;
  justify-content: space-between;
  align-items: center;
  cursor: pointer;
  user-select: none;
}

.dropdown-icon {
  transition: transform 0.2s ease;
  color: var(--text-tertiary);
}

.rotate-180 {
  transform: rotate(180deg);
}

.custom-select-options {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  width: 100%;
  background: var(--bg-primary);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-lg);
  padding: 4px;
  margin: 0;
  list-style: none;
  z-index: 50;
  max-height: 250px;
  overflow-y: auto;
}

.custom-select-option {
  padding: 8px 12px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: var(--text-sm);
  color: var(--text-secondary);
  transition: all 0.2s;
}

.custom-select-option:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.custom-select-option.is-selected {
  background: var(--bg-active);
  color: var(--text-primary);
  font-weight: 500;
}

.dropdown-fade-enter-active,
.dropdown-fade-leave-active {
  transition: opacity 0.2s, transform 0.2s;
}

.dropdown-fade-enter-from,
.dropdown-fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
