import { Activity, ClipboardList, Crosshair, LayoutGrid, Scale } from 'lucide-vue-next';

export const DEFAULT_WORK_VIEW = 'overview';

export const WORK_VIEW_ITEMS = Object.freeze([
  { id: 'overview', labelKey: 'work.mobile_overview', icon: LayoutGrid },
  { id: 'goals', labelKey: 'work.goals', icon: Crosshair },
  { id: 'tasks', labelKey: 'work.tasks', icon: ClipboardList },
  { id: 'decisions', labelKey: 'work.decisions', icon: Scale },
  { id: 'activity', labelKey: 'work.mobile_activity', icon: Activity },
]);

export function isWorkView(value) {
  return WORK_VIEW_ITEMS.some(item => item.id === value);
}
