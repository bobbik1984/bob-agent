<script setup>
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';

const props = defineProps({
  mood: {
    type: String,
    default: 'idle',
    validator: val => ['idle', 'thinking', 'speaking', 'needs_human'].includes(val),
  },
  size: {
    type: String,
    default: 'hero',
    validator: val => ['hero', 'compact'].includes(val),
  },
  interactive: {
    type: Boolean,
    default: true,
  },
  badgeCount: {
    type: Number,
    default: 0,
  },
});

const emit = defineEmits(['activate']);

const { t } = useI18n();

const sizeClass = computed(() => (props.size === 'compact' ? 'orb-compact size-compact' : 'orb-hero size-hero'));

const moodClass = computed(() => {
  switch (props.mood) {
    case 'thinking':
      return 'mood-thinking';
    case 'speaking':
      return 'mood-speaking';
    case 'needs_human':
      return 'mood-needs_human mood-needs-human';
    case 'idle':
    default:
      return 'mood-idle';
  }
});

const ariaLabel = computed(() => {
  let label = '';
  switch (props.mood) {
    case 'thinking':
      label = t('jarvisOrbThinking');
      break;
    case 'speaking':
      label = t('jarvisOrbSpeaking');
      break;
    case 'needs_human':
      label = t('jarvisOrbNeedsHuman');
      break;
    case 'idle':
    default:
      label = t('jarvisOrbIdle');
      break;
  }
  if (props.badgeCount > 0) {
    label += ` (${props.badgeCount})`;
  }
  return label;
});

function onActivate(event) {
  if (!props.interactive) return;
  emit('activate', event);
}
</script>

<template>
  <button
    v-if="interactive"
    type="button"
    class="jarvis-orb"
    :class="[sizeClass, moodClass]"
    :aria-label="ariaLabel"
    @click="onActivate"
    @keydown.enter.prevent="onActivate"
    @keydown.space.prevent="onActivate"
  >
    <div class="orb-visual">
      <div class="orb-halo" aria-hidden="true"></div>
      <div v-if="mood === 'speaking'" class="orb-waves" aria-hidden="true">
        <span class="orb-wave-ring wave-1"></span>
        <span class="orb-wave-ring wave-2"></span>
      </div>
      <div class="orb-sphere" aria-hidden="true">
        <div class="orb-mesh"></div>
        <div class="orb-swirl"></div>
        <div class="orb-sheen"></div>
      </div>
      <span
        v-if="badgeCount > 0"
        class="orb-badge"
        aria-hidden="true"
      >{{ badgeCount > 99 ? '99+' : badgeCount }}</span>
    </div>
  </button>
  <div
    v-else
    class="jarvis-orb"
    :class="[sizeClass, moodClass]"
    role="status"
    :aria-label="ariaLabel"
  >
    <div class="orb-visual">
      <div class="orb-halo" aria-hidden="true"></div>
      <div v-if="mood === 'speaking'" class="orb-waves" aria-hidden="true">
        <span class="orb-wave-ring wave-1"></span>
        <span class="orb-wave-ring wave-2"></span>
      </div>
      <div class="orb-sphere" aria-hidden="true">
        <div class="orb-mesh"></div>
        <div class="orb-swirl"></div>
        <div class="orb-sheen"></div>
      </div>
      <span
        v-if="badgeCount > 0"
        class="orb-badge"
        aria-hidden="true"
      >{{ badgeCount > 99 ? '99+' : badgeCount }}</span>
    </div>
  </div>
</template>

<style scoped>
.jarvis-orb {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  padding: 0;
  margin: 0;
  cursor: default;
  outline: none;
  user-select: none;
  -webkit-tap-highlight-color: transparent;
}

button.jarvis-orb {
  cursor: pointer;
}

button.jarvis-orb:focus-visible {
  outline: 2px solid var(--accent-primary);
  outline-offset: 4px;
  border-radius: var(--radius-pill);
}

.orb-visual {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
}

/* Sizing */
.jarvis-orb.size-hero,
.jarvis-orb.orb-hero {
  width: 6.5rem;
  height: 6.5rem;
}

.jarvis-orb.size-compact,
.jarvis-orb.orb-compact {
  width: 2.125rem;
  height: 2.125rem;
}

/* Outer Ambient Glow / Halo */
.orb-halo {
  position: absolute;
  inset: -15%;
  border-radius: var(--radius-pill);
  background: radial-gradient(
    circle,
    var(--orb-glow-soft) 0%,
    color-mix(in srgb, var(--orb-core-violet) 25%, transparent) 40%,
    color-mix(in srgb, var(--orb-core-cyan) 15%, transparent) 70%,
    transparent 100%
  );
  filter: blur(12px);
  pointer-events: none;
  opacity: 0.75;
  transition: opacity 0.4s ease, filter 0.4s ease;
}

.jarvis-orb.mood-needs_human .orb-halo,
.jarvis-orb.mood-needs-human .orb-halo {
  background: radial-gradient(
    circle,
    var(--orb-amber-glow) 0%,
    color-mix(in srgb, var(--orb-amber-core) 30%, transparent) 50%,
    transparent 100%
  );
  opacity: 0.95;
  filter: blur(14px);
}

/* Spherical Core Container */
.orb-sphere {
  position: relative;
  width: 100%;
  height: 100%;
  border-radius: var(--radius-pill);
  overflow: hidden;
  box-shadow: 0 4px 16px color-mix(in srgb, var(--line) 40%, transparent);
  background: var(--bg-primary);
}

/* Broad overlapping colours avoid hard wedges and dark seams. */
.orb-mesh {
  position: absolute;
  inset: -12%;
  border-radius: var(--radius-pill);
  background:
    radial-gradient(ellipse at 24% 22%, var(--orb-core-cyan), transparent 72%),
    radial-gradient(ellipse at 78% 80%, var(--orb-core-magenta), transparent 75%),
    linear-gradient(140deg in oklab, var(--orb-core-cyan), var(--orb-core-blue) 38%, var(--orb-core-violet) 68%, var(--orb-core-magenta));
  filter: blur(8px);
  transition: background 0.6s ease;
}

.jarvis-orb.mood-needs_human .orb-mesh,
.jarvis-orb.mood-needs-human .orb-mesh {
  background:
    radial-gradient(circle at 50% 40%, var(--orb-amber-core) 0%, transparent 70%),
    radial-gradient(circle at 40% 70%, var(--orb-amber-glow) 0%, transparent 60%),
    var(--bg-primary);
  filter: blur(3px);
}

/* Layer 2: Conic Fluid Swirl */
.orb-swirl {
  position: absolute;
  inset: -20%;
  border-radius: var(--radius-pill);
  background: conic-gradient(
    from 0deg,
    var(--orb-core-cyan),
    var(--orb-core-violet),
    var(--orb-core-magenta),
    var(--orb-core-blue),
    var(--orb-core-cyan)
  );
  opacity: 0.18;
  mix-blend-mode: normal;
  filter: blur(16px);
}

.jarvis-orb.mood-needs_human .orb-swirl,
.jarvis-orb.mood-needs-human .orb-swirl {
  background: conic-gradient(
    from 0deg,
    var(--orb-amber-core),
    var(--orb-amber-glow),
    var(--color-warning),
    var(--orb-amber-core)
  );
  opacity: 0.35;
}

/* Layer 3: Glass Sheen & Surface Ring */
.orb-sheen {
  position: absolute;
  inset: 0;
  border-radius: var(--radius-pill);
  box-shadow:
    inset 0 0 0 1px var(--orb-surface-ring),
    inset 0 3px 12px color-mix(in srgb, var(--orb-surface-ring) 50%, transparent),
    inset 0 -2px 6px color-mix(in srgb, CanvasText 10%, transparent);
  pointer-events: none;
}

/* Mood 1: Idle Breathing */
@keyframes orb-breathe {
  0%, 100% {
    transform: scale(0.98);
    opacity: 0.92;
  }
  50% {
    transform: scale(1.02);
    opacity: 1;
  }
}

@keyframes orb-spin {
  0% { transform: rotate(0deg); }
  100% { transform: rotate(360deg); }
}

.jarvis-orb.mood-idle .orb-visual {
  animation: orb-breathe 4s ease-in-out infinite;
}

.jarvis-orb.mood-idle .orb-swirl {
  animation: orb-spin 24s linear infinite;
}

/* Mood 2: Thinking Fluid Swirl & Ripple */
@keyframes orb-think-pulse {
  0%, 100% {
    transform: scale(0.97);
    filter: brightness(1);
  }
  50% {
    transform: scale(1.03);
    filter: brightness(1.15);
  }
}

@keyframes orb-mesh-ripple {
  0% { transform: scale(0.95) rotate(-5deg); }
  100% { transform: scale(1.05) rotate(5deg); }
}

.jarvis-orb.mood-thinking .orb-visual {
  animation: orb-think-pulse 2s ease-in-out infinite;
}

.jarvis-orb.mood-thinking .orb-swirl {
  animation: orb-spin 3s linear infinite;
  opacity: 0.7;
}

.jarvis-orb.mood-thinking .orb-mesh {
  animation: orb-mesh-ripple 2.5s ease-in-out infinite alternate;
}

/* Mood 3: Speaking Voice Wave Rings */
@keyframes orb-speak-pulse {
  0%, 100% { transform: scale(0.97); }
  35% { transform: scale(1.04); }
  70% { transform: scale(0.99); }
}

.jarvis-orb.mood-speaking .orb-visual {
  animation: orb-speak-pulse 1.1s ease-in-out infinite;
}

.orb-waves {
  position: absolute;
  inset: 0;
  pointer-events: none;
  display: flex;
  align-items: center;
  justify-content: center;
}

.orb-wave-ring {
  position: absolute;
  inset: -5%;
  border-radius: var(--radius-pill);
  border: 1.5px solid var(--orb-core-cyan);
  opacity: 0;
  animation: orb-wave-expand 1.8s cubic-bezier(0.2, 0.6, 0.35, 1) infinite;
}

.orb-wave-ring.wave-2 {
  animation-delay: 0.9s;
  border-color: var(--orb-core-violet);
}

@keyframes orb-wave-expand {
  0% {
    transform: scale(0.95);
    opacity: 0.65;
  }
  100% {
    transform: scale(1.45);
    opacity: 0;
  }
}

/* Mood 4: Needs Human Amber Beacon */
@keyframes orb-amber-beacon {
  0%, 100% {
    transform: scale(0.97);
    filter: drop-shadow(0 0 6px var(--orb-amber-glow));
  }
  50% {
    transform: scale(1.04);
    filter: drop-shadow(0 0 14px var(--orb-amber-core));
  }
}

.jarvis-orb.mood-needs_human .orb-visual,
.jarvis-orb.mood-needs-human .orb-visual {
  animation: orb-amber-beacon 1.6s ease-in-out infinite;
}

/* Semantic Attention Badge */
.orb-badge {
  position: absolute;
  top: -2px;
  right: -2px;
  min-width: 1.125rem;
  height: 1.125rem;
  padding: 0 0.3rem;
  border-radius: var(--radius-pill);
  background: var(--color-error);
  color: var(--text-on-accent);
  font-size: var(--text-2xs);
  font-weight: 700;
  line-height: 1.125rem;
  text-align: center;
  box-shadow: 0 0 0 1.5px var(--bg-primary);
  pointer-events: none;
  z-index: 2;
}

.jarvis-orb.size-compact .orb-badge,
.jarvis-orb.orb-compact .orb-badge {
  top: -4px;
  right: -4px;
  min-width: 0.875rem;
  height: 0.875rem;
  line-height: 0.875rem;
  padding: 0 0.2rem;
  font-size: 0.6rem;
}

/* Reduced Motion Fallback */
@media (prefers-reduced-motion: reduce) {
  .jarvis-orb,
  .jarvis-orb *,
  .orb-visual,
  .orb-swirl,
  .orb-mesh,
  .orb-wave-ring {
    animation: none !important;
    transition: none !important;
  }
  .orb-wave-ring {
    display: none !important;
  }
  .orb-halo {
    opacity: 0.6 !important;
    transform: none !important;
  }
}
</style>
