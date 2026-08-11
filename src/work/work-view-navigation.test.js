import { describe, expect, it } from 'vitest';
import { DEFAULT_WORK_VIEW, isWorkView, WORK_VIEW_ITEMS } from './work-view-navigation.js';

describe('shared work navigation', () => {
  it('provides one ordered menu for the desktop sidebar and compact top bar', () => {
    expect(WORK_VIEW_ITEMS.map(item => item.id)).toEqual([
      'overview', 'goals', 'tasks', 'decisions', 'activity',
    ]);
    expect(DEFAULT_WORK_VIEW).toBe('overview');
  });

  it('accepts only registered work views', () => {
    expect(isWorkView('tasks')).toBe(true);
    expect(isWorkView('unknown')).toBe(false);
  });
});
