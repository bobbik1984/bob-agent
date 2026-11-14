<script>
const THEME_VAR_KEYS = [
  '--bg-primary',
  '--bg-secondary',
  '--surface',
  '--line',
  '--text-primary',
  '--text-secondary',
  '--accent-primary',
  '--color-success',
  '--color-warning',
  '--color-error',
];

const DEFAULT_THEME_FALLBACKS = {
  '--bg-primary': 'Canvas',
  '--bg-secondary': 'color-mix(in srgb, CanvasText 4.5%, Canvas)',
  '--surface': 'Canvas',
  '--line': 'color-mix(in srgb, CanvasText 12%, Canvas)',
  '--text-primary': 'CanvasText',
  '--text-secondary': 'GrayText',
  '--accent-primary': 'Highlight',
  '--color-success': 'Highlight',
  '--color-warning': 'CanvasText',
  '--color-error': 'CanvasText',
};

export function extractThemeVars() {
  const vars = {};
  if (typeof document !== 'undefined' && typeof window !== 'undefined' && window.getComputedStyle) {
    try {
      const style = window.getComputedStyle(document.documentElement);
      for (const k of THEME_VAR_KEYS) {
        const val = style.getPropertyValue(k)?.trim();
        if (val) {
          vars[k] = val;
        }
      }
    } catch {}
  }
  return vars;
}

export function computeFeatureSrcdocKey(feature = {}) {
  return `${feature?.name || ''}::${feature?.version ?? 0}::${feature?.widget_html || ''}`;
}

export function buildFeatureWidgetSrcdoc(feature = {}, themeVars = {}) {
  const name = String(feature?.name || 'widget');
  const rawHtml = String(feature?.widget_html || '');
  const initialState = feature?.cached_state !== undefined ? feature.cached_state : null;
  const initialStorage =
    feature?.storage && typeof feature.storage === 'object' ? feature.storage : {};

  const safeStateJson = JSON.stringify(initialState).replace(/<\/script/gi, '<\\/script');
  const safeStorageJson = JSON.stringify(initialStorage).replace(/<\/script/gi, '<\\/script');
  const safePkgJson = JSON.stringify(name).replace(/<\/script/gi, '<\\/script');

  const cssRules = [];
  for (const k of THEME_VAR_KEYS) {
    const val = themeVars[k] || DEFAULT_THEME_FALLBACKS[k];
    if (val) {
      cssRules.push(`${k}: ${val};`);
    }
  }

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <style>
    :root {
      ${cssRules.join('\n      ')}
      color-scheme: light dark;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    }
    *, *::before, *::after {
      box-sizing: border-box;
    }
    body {
      margin: 0;
      padding: 0;
      background: transparent;
      color: var(--text-primary);
      overflow-x: hidden;
      overflow-y: auto;
    }
    @scope (.bob-widget-scope) {
      :scope {
        display: block;
        width: 100%;
        min-height: 100%;
        box-sizing: border-box;
      }
    }
  </style>
  <script>
    (function() {
      const PKG_NAME = ${safePkgJson};
      const stateListeners = [];
      const storageListeners = [];
      const themeListeners = [];
      const pendingCalls = new Map();

      function postRuntimeError(rawMsg) {
        try {
          window.parent.postMessage({
            type: 'bob.error',
            pkg: PKG_NAME,
            message: String(rawMsg || 'Widget runtime error')
          }, '*');
        } catch (e) {}
      }

      window.addEventListener('error', function(ev) {
        const msg = ev && (ev.error && ev.error.message ? ev.error.message : ev.message);
        postRuntimeError(msg || 'Uncaught widget script error');
      });

      window.addEventListener('unhandledrejection', function(ev) {
        const reason = ev && ev.reason;
        const msg = reason && reason.message ? reason.message : String(reason || 'Unhandled promise rejection');
        postRuntimeError(msg);
      });

      const bobBridge = {
        state: ${safeStateJson},
        storage: ${safeStorageJson},
        call: function(route, payload) {
          return new Promise(function(resolve, reject) {
            const callId = 'c_' + Math.random().toString(36).slice(2) + '_' + Date.now();
            pendingCalls.set(callId, { resolve: resolve, reject: reject });
            try {
              window.parent.postMessage({
                type: 'bob.call',
                pkg: PKG_NAME,
                callId: callId,
                route: route,
                payload: payload || {}
              }, '*');
            } catch (err) {
              pendingCalls.delete(callId);
              reject(err);
            }
          });
        },
        save: function(nextStorage) {
          return new Promise(function(resolve, reject) {
            const str = JSON.stringify(nextStorage || {});
            if (str.length > 8192) {
              reject(new Error('FEATURE_STORAGE_LIMIT_EXCEEDED'));
              return;
            }
            bobBridge.storage = nextStorage;
            const callId = 's_' + Math.random().toString(36).slice(2) + '_' + Date.now();
            pendingCalls.set(callId, { resolve: resolve, reject: reject });
            try {
              window.parent.postMessage({
                type: 'bob.save',
                pkg: PKG_NAME,
                callId: callId,
                storage: nextStorage
              }, '*');
            } catch (err) {
              pendingCalls.delete(callId);
              reject(err);
            }
          });
        },
        onState: function(fn) {
          if (typeof fn === 'function') stateListeners.push(fn);
        },
        onStorage: function(fn) {
          if (typeof fn === 'function') storageListeners.push(fn);
        },
        onTheme: function(fn) {
          if (typeof fn === 'function') themeListeners.push(fn);
        }
      };

      window.bob = bobBridge;

      window.addEventListener('message', function(ev) {
        const data = ev.data;
        if (!data || typeof data !== 'object') return;
        if (data.type === 'bob.call.result' && data.callId) {
          const entry = pendingCalls.get(data.callId);
          if (entry) {
            pendingCalls.delete(data.callId);
            if (data.ok) entry.resolve(data.data);
            else entry.reject(new Error(data.error || 'CALL_FAILED'));
          }
        } else if (data.type === 'bob.save.result' && data.callId) {
          const entry = pendingCalls.get(data.callId);
          if (entry) {
            pendingCalls.delete(data.callId);
            if (data.ok) entry.resolve(true);
            else entry.reject(new Error(data.error || 'SAVE_FAILED'));
          }
        } else if (data.type === 'bob.state') {
          bobBridge.state = data.state;
          for (let i = 0; i < stateListeners.length; i++) {
            try { stateListeners[i](data.state); } catch (e) {}
          }
        } else if (data.type === 'bob.storage') {
          bobBridge.storage = data.storage && typeof data.storage === 'object' ? data.storage : {};
          for (let i = 0; i < storageListeners.length; i++) {
            try { storageListeners[i](bobBridge.storage); } catch (e) {}
          }
        } else if (data.type === 'bob-theme') {
          if (data.vars && typeof data.vars === 'object') {
            for (const k in data.vars) {
              if (Object.prototype.hasOwnProperty.call(data.vars, k)) {
                document.documentElement.style.setProperty(k, data.vars[k]);
              }
            }
          }
          for (let i = 0; i < themeListeners.length; i++) {
            try { themeListeners[i](data.vars); } catch (e) {}
          }
        }
      });

      function reportHeight() {
        const scopeEl = document.querySelector('.bob-widget-scope') || document.body;
        const rect = scopeEl.getBoundingClientRect();
        const height = Math.ceil(Math.max(rect.height, scopeEl.scrollHeight, document.documentElement.scrollHeight));
        window.parent.postMessage({
          type: 'bob.resize',
          pkg: PKG_NAME,
          height: height
        }, '*');
      }

      if (typeof ResizeObserver !== 'undefined') {
        const ro = new ResizeObserver(function() {
          reportHeight();
        });
        window.addEventListener('DOMContentLoaded', function() {
          const el = document.querySelector('.bob-widget-scope') || document.body;
          ro.observe(el);
          reportHeight();
        });
      } else {
        window.addEventListener('DOMContentLoaded', reportHeight);
        window.addEventListener('load', reportHeight);
      }
    })();
  <\/script>
</head>
<body>
  <div class="bob-widget-scope">
${rawHtml}
  </div>
</body>
</html>`;
}
</script>

<script setup>
import { computed, inject, onMounted, onUnmounted, ref, watch } from 'vue';

const props = defineProps({
  feature: {
    type: Object,
    required: true,
  },
});

const shell = inject('bobAppShell', {});
const jarvisHome = computed(() => shell.jarvisHome);
const tr = (key, fallback) => (typeof shell.t === 'function' ? shell.t(key) : fallback);

const iframeRef = ref(null);
const frameHeight = ref(180);
const reloadNonce = ref(0);
const widgetRuntimeError = ref('');

const currentThemeVars = ref(extractThemeVars());

// Zero-flicker srcdoc key: only regenerate iframe HTML when identity, version, HTML template, or manual retry changes.
const srcdocKey = computed(
  () => `${props.feature?.name || ''}::${props.feature?.version ?? 0}::${props.feature?.widget_html || ''}`,
);
const composedSrcdoc = ref('');

watch(
  [srcdocKey, reloadNonce],
  () => {
    widgetRuntimeError.value = '';
    composedSrcdoc.value = buildFeatureWidgetSrcdoc(props.feature, currentThemeVars.value);
  },
  { immediate: true },
);

async function handleWindowMessage(event) {
  if (!iframeRef.value?.contentWindow) return;
  if (event.source !== iframeRef.value.contentWindow) return;
  if (!event.data || event.data.pkg !== props.feature.name) return;

  const data = event.data;
  if (data.type === 'bob.call') {
    const { callId, route, payload } = data;
    try {
      let res = null;
      if (typeof jarvisHome.value?.callFeatureRoute === 'function') {
        res = await jarvisHome.value.callFeatureRoute(props.feature.name, route, payload);
      }
      iframeRef.value?.contentWindow?.postMessage(
        {
          type: 'bob.call.result',
          callId,
          ok: true,
          data: res,
        },
        '*',
      );
    } catch (err) {
      iframeRef.value?.contentWindow?.postMessage(
        {
          type: 'bob.call.result',
          callId,
          ok: false,
          error: String(err?.message || err),
        },
        '*',
      );
    }
  } else if (data.type === 'bob.save') {
    const { callId, storage } = data;
    try {
      if (typeof jarvisHome.value?.saveFeatureStorage === 'function') {
        await jarvisHome.value.saveFeatureStorage(props.feature.name, storage);
      }
      iframeRef.value?.contentWindow?.postMessage(
        {
          type: 'bob.save.result',
          callId,
          ok: true,
        },
        '*',
      );
    } catch (err) {
      iframeRef.value?.contentWindow?.postMessage(
        {
          type: 'bob.save.result',
          callId,
          ok: false,
          error: String(err?.message || err),
        },
        '*',
      );
    }
  } else if (data.type === 'bob.resize') {
    const h = Number(data.height) || 180;
    frameHeight.value = Math.max(110, Math.min(420, h));
  } else if (data.type === 'bob.error') {
    const errMsg = String(data.message || 'Widget runtime error');
    widgetRuntimeError.value = errMsg;
    if (typeof jarvisHome.value?.reportFeatureError === 'function') {
      await jarvisHome.value.reportFeatureError(props.feature.name, errMsg);
    }
  }
}

function handleRetryWidget() {
  widgetRuntimeError.value = '';
  reloadNonce.value += 1;
}

async function handleRollbackWidget() {
  widgetRuntimeError.value = '';
  if (typeof jarvisHome.value?.rollbackFeature === 'function') {
    await jarvisHome.value.rollbackFeature(props.feature.name);
  }
}

watch(
  () => props.feature?.cached_state,
  nextState => {
    if (iframeRef.value?.contentWindow) {
      iframeRef.value.contentWindow.postMessage(
        {
          type: 'bob.state',
          state: nextState,
        },
        '*',
      );
    }
  },
  { deep: true },
);

watch(
  () => props.feature?.storage,
  nextStorage => {
    if (iframeRef.value?.contentWindow) {
      iframeRef.value.contentWindow.postMessage(
        {
          type: 'bob.storage',
          storage: nextStorage && typeof nextStorage === 'object' ? nextStorage : {},
        },
        '*',
      );
    }
  },
  { deep: true },
);

function pushThemeUpdate() {
  currentThemeVars.value = extractThemeVars();
  if (iframeRef.value?.contentWindow) {
    iframeRef.value.contentWindow.postMessage(
      {
        type: 'bob-theme',
        vars: currentThemeVars.value,
      },
      '*',
    );
  }
}

let observer = null;
onMounted(() => {
  if (typeof window !== 'undefined') {
    window.addEventListener('message', handleWindowMessage);
    if (typeof MutationObserver !== 'undefined' && document.documentElement) {
      observer = new MutationObserver(pushThemeUpdate);
      observer.observe(document.documentElement, {
        attributes: true,
        attributeFilter: ['class', 'data-theme', 'style'],
      });
    }
  }
});

onUnmounted(() => {
  if (typeof window !== 'undefined') {
    window.removeEventListener('message', handleWindowMessage);
  }
  if (observer) {
    observer.disconnect();
    observer = null;
  }
});
</script>

<template>
  <article class="feature-widget-card" :aria-label="feature.title || feature.name">
    <header class="feature-widget-header">
      <div class="feature-widget-title-wrap">
        <span class="feature-widget-badge">v{{ feature.version || 1 }}</span>
        <h3 class="feature-widget-title">{{ feature.title || feature.name }}</h3>
      </div>
      <div v-if="feature.status === 'trial'" class="feature-trial-indicator">
        <span class="trial-dot" aria-hidden="true"></span>
        <span class="trial-label">Trial</span>
      </div>
    </header>

    <div
      v-if="widgetRuntimeError"
      class="feature-widget-error-strip"
      role="alert"
    >
      <div class="widget-error-copy">
        <svg class="inline-svg-xs" viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="12" cy="12" r="10" />
          <line x1="12" y1="8" x2="12" y2="12" />
          <line x1="12" y1="16" x2="12.01" y2="16" />
        </svg>
        <span class="widget-error-text">
          {{ tr('featureWidgetRuntimeError', 'Widget encountered a runtime error') }}:
          {{ widgetRuntimeError }}
        </span>
      </div>
      <div class="widget-error-actions">
        <button
          type="button"
          class="widget-error-btn"
          @click="handleRetryWidget"
        >
          {{ tr('featureWidgetRetryBtn', 'Retry') }}
        </button>
        <button
          type="button"
          class="widget-error-btn rollback"
          @click="handleRollbackWidget"
        >
          {{ tr('featureWidgetRollbackBtn', 'Rollback') }}
        </button>
      </div>
    </div>

    <iframe
      ref="iframeRef"
      class="feature-widget-iframe"
      sandbox="allow-scripts"
      referrerpolicy="no-referrer"
      :srcdoc="composedSrcdoc"
      :title="feature.title || feature.name"
      :style="{ height: frameHeight + 'px' }"
    />
  </article>
</template>

<style scoped>
.feature-widget-card {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100%;
  border-radius: var(--radius-md);
  background: var(--surface, var(--bg-primary));
  border: 1px solid var(--line);
  overflow: hidden;
  transition: border-color var(--transition-fast, 0.15s ease);
}

.feature-widget-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--gap-xs) var(--gap-sm);
  border-bottom: 1px solid var(--line);
  background: var(--bg-secondary);
}

.feature-widget-title-wrap {
  display: flex;
  align-items: center;
  gap: var(--gap-xs);
  min-width: 0;
}

.feature-widget-badge {
  display: inline-flex;
  align-items: center;
  padding: 1px var(--gap-3xs);
  font-size: var(--text-2xs);
  font-weight: 600;
  border-radius: var(--radius-xs);
  background: var(--bg-primary);
  color: var(--accent-primary);
  border: 1px solid var(--line);
}

.feature-widget-title {
  margin: 0;
  font-size: var(--text-xs);
  font-weight: 600;
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.feature-trial-indicator {
  display: inline-flex;
  align-items: center;
  gap: var(--gap-3xs);
  font-size: var(--text-2xs);
  color: var(--color-warning);
}

.trial-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--color-warning);
}

.trial-label {
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.feature-widget-error-strip {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: var(--gap-xs);
  padding: var(--gap-xs) var(--gap-sm);
  border-bottom: 1px solid var(--line);
  background: var(--error-soft, var(--bg-secondary));
  color: var(--error-text, var(--color-error));
  font-size: var(--text-2xs);
}

.widget-error-copy {
  display: flex;
  align-items: center;
  gap: var(--gap-2xs);
  min-width: 0;
  flex: 1;
}

.widget-error-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.widget-error-actions {
  display: flex;
  align-items: center;
  gap: var(--gap-2xs);
  flex-shrink: 0;
}

.widget-error-btn {
  min-height: 1.5rem;
  padding: 0.15rem 0.45rem;
  font-size: var(--text-2xs);
  border-radius: var(--radius-xs);
  border: 1px solid var(--line);
  background: var(--bg-primary);
  color: var(--text-primary);
}

.widget-error-btn.rollback:hover {
  border-color: var(--color-error);
  color: var(--color-error);
}

.feature-widget-iframe {
  display: block;
  width: 100%;
  border: none;
  background: transparent;
  transition: height 0.2s ease;
}
</style>
