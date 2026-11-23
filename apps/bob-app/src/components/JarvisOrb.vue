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

/* Water surface: restrained refraction and a moving specular reflection. */
.orb-halo {
  position: absolute; inset: 6%; border-radius: 50%;
  background: var(--water-shadow); filter: blur(18px); opacity: .25;
  transform: translateY(12%); pointer-events: none;
}
.orb-sphere {
  position: relative; width: 100%; height: 100%; overflow: hidden;
  border-radius: 50%;
  background: var(--water-body);
  box-shadow: inset 1px 2px 3px var(--water-highlight), inset -4px -6px 10px var(--water-edge), 0 8px 22px var(--water-shadow);
  transform: scaleX(var(--water-strain)) scaleY(calc(1 / var(--water-strain)));
  animation: water-tension 9s ease-in-out infinite;
}
.orb-mesh {
  position: absolute; inset: 0; border-radius: inherit;
  background:
    radial-gradient(ellipse at 28% 20%, var(--water-highlight), transparent 34%),
    radial-gradient(ellipse at 72% 78%, var(--water-highlight), transparent 38%),
    radial-gradient(ellipse at 54% 52%, transparent 38%, var(--water-edge) 76%, var(--water-highlight) 99%);
}
.orb-swirl {
  position: absolute; inset: 10%; border-radius: 50%; opacity: .3;
  background: radial-gradient(ellipse at 70% 75%, var(--water-caustic), transparent 55%);
  filter: blur(5px);
}
.orb-sheen {
  position: absolute; inset: 5%; border-radius: inherit; pointer-events: none;
  border-top: 2px solid var(--water-highlight);
  border-left: 1px solid var(--water-highlight);
  transform: rotate(-28deg);
}
.orb-sheen::after {
  content: ''; position: absolute; width: 22%; height: 9%; top: 12%; left: 26%;
  border-radius: 50%; background: var(--water-highlight); filter: blur(2px);
}
/* One interpolated strain with its reciprocal keeps projected area constant
   throughout the animation, not just at keyframes. This is a surface-tension
   visual approximation, not a fluid simulation. */
@property --water-strain {
  syntax: '<number>';
  inherits: false;
  initial-value: 1;
}
@keyframes water-tension {
  0%, 18%, 100% { --water-strain: 1; }
  32% { --water-strain: .975; }
  46% { --water-strain: 1.018; }
  60% { --water-strain: .992; }
  74% { --water-strain: 1.004; }
  88% { --water-strain: 1; }
}
.mood-thinking .orb-sphere { animation-duration: 4s; }
.mood-needs_human .orb-mesh, .mood-needs-human .orb-mesh {
  background: radial-gradient(ellipse at 70% 78%, var(--orb-amber-glow), transparent 65%);
}

/* State changes alter tension cadence, never inflate the whole drop. */
.jarvis-orb.mood-speaking .orb-sphere { animation-duration: 5s; }
.jarvis-orb.mood-thinking .orb-swirl { opacity: .45; }
.orb-waves { display: none; }
.jarvis-orb.mood-needs_human .orb-halo,
.jarvis-orb.mood-needs-human .orb-halo { background: var(--orb-amber-glow); }

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
