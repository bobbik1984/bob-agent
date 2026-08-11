export const WORK_MOBILE_TABS = Object.freeze(['overview', 'goals', 'tasks', 'decisions', 'activity']);

const CLOSED_TASK_STATUSES = new Set(['done', 'cancelled', 'archived']);

export function sortProjectsByUpdatedAt(projects = []) {
  return [...projects].sort((left, right) => {
    const rightTime = Date.parse(right?.updatedAt || '') || 0;
    const leftTime = Date.parse(left?.updatedAt || '') || 0;
    return rightTime - leftTime;
  });
}

export function getOpenTasks(aggregate) {
  return (aggregate?.tasks || []).filter(task => !CLOSED_TASK_STATUSES.has(task.status));
}

export function getMobileProjectCounts(aggregate) {
  const goals = aggregate?.goals?.length || 0;
  const tasks = getOpenTasks(aggregate).length;
  const decisions = aggregate?.decisions?.length || 0;
  const risks = aggregate?.risks?.length || 0;
  const activity = aggregate?.recentEvents?.length || 0;
  return { goals, tasks, decisions, risks, activity, overview: goals + tasks + decisions + risks };
}

export function toggleExpandedProjectIds(currentIds, projectId) {
  const nextIds = new Set(currentIds || []);
  if (nextIds.has(projectId)) nextIds.delete(projectId);
  else nextIds.add(projectId);
  return nextIds;
}

