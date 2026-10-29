/** Draw compact independently highlighted arrows for widget sorting controls. */
export function createSortIcon() {
  const icon = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  icon.classList.add('sort-arrows');
  icon.setAttribute('viewBox', '0 0 20 20');
  icon.setAttribute('width', '18');
  icon.setAttribute('height', '18');
  icon.setAttribute('fill', 'none');
  icon.setAttribute('stroke', 'currentColor');
  icon.setAttribute('stroke-width', '1.5');
  icon.setAttribute('stroke-linecap', 'round');
  icon.setAttribute('stroke-linejoin', 'round');
  icon.setAttribute('aria-hidden', 'true');
  icon.setAttribute('focusable', 'false');
  for (const [direction, shape] of [
    ['asc', 'M6 16V4m-2.5 2.5L6 4l2.5 2.5'],
    ['desc', 'M14 4v12m-2.5-2.5L14 16l2.5-2.5'],
  ] as const) {
    const arrow = document.createElementNS(icon.namespaceURI, 'path');
    arrow.setAttribute('data-arrow', direction);
    arrow.setAttribute('d', shape);
    icon.append(arrow);
  }
  return icon;
}
