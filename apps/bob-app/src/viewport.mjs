// Visual viewport follows Android's keyboard without scrolling the entire app.
export function bindViewport(win, root) {
  const viewport = win.visualViewport;
  const update = () => {
    const height = viewport?.height || win.innerHeight;
    const top = viewport?.offsetTop || 0;
    root.style.setProperty('--viewport-height', `${height}px`);
    root.style.setProperty('--viewport-top', `${top}px`);
  };
  update();
  viewport?.addEventListener('resize', update);
  viewport?.addEventListener('scroll', update);
  win.addEventListener('resize', update);
  return () => {
    viewport?.removeEventListener('resize', update);
    viewport?.removeEventListener('scroll', update);
    win.removeEventListener('resize', update);
    root.style.removeProperty('--viewport-height');
    root.style.removeProperty('--viewport-top');
  };
}
