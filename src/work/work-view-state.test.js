import { describe, expect, it } from 'vitest';
import { getMobileProjectCounts, getOpenTasks, sortProjectsByUpdatedAt, toggleExpandedProjectIds } from './work-view-state.js';

describe('shared work view helpers', () => {
  it('sorts projects by latest update without mutating input', () => {
    const projects = [
      { id: 'old', updatedAt: '2026-08-01T08:00:00Z' },
      { id: 'new', updatedAt: '2026-08-11T08:00:00Z' },
    ];
    expect(sortProjectsByUpdatedAt(projects).map(project => project.id)).toEqual(['new', 'old']);
    expect(projects.map(project => project.id)).toEqual(['old', 'new']);
  });

  it('counts only actionable tasks as open', () => {
    const aggregate = { tasks: [
      { status: 'pending' }, { status: 'active' }, { status: 'done' },
      { status: 'cancelled' }, { status: 'archived' },
    ] };
    expect(getOpenTasks(aggregate)).toHaveLength(2);
  });

  it('builds stable counts for every mobile tab', () => {
    expect(getMobileProjectCounts({
      goals: [{}, {}], tasks: [{ status: 'pending' }, { status: 'done' }],
      decisions: [{}], risks: [{}, {}], recentEvents: [{}, {}, {}],
    })).toEqual({ goals: 2, tasks: 1, decisions: 1, risks: 2, activity: 3, overview: 6 });
  });

  it('allows several projects to remain independently expanded', () => {
    const first = toggleExpandedProjectIds(new Set(), 'a');
    const second = toggleExpandedProjectIds(first, 'b');
    const third = toggleExpandedProjectIds(second, 'a');
    expect([...second]).toEqual(['a', 'b']);
    expect([...third]).toEqual(['b']);
  });
});
