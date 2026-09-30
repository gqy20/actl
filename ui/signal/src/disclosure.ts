/** Native host receives final geometry once, never animation-frame heights. */
export function disclosureMotion(content: HTMLElement, reportTarget: (height: number) => void): { isAnimating(): boolean } {
  const motions = new Map<HTMLDetailsElement, { animation: Animation; body: HTMLElement; target: number; expanded: boolean }>();
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  const finalHeight = (): number => {
    let height = content.getBoundingClientRect().height;
    for (const motion of motions.values()) height += motion.target - motion.body.getBoundingClientRect().height;
    return Math.ceil(height);
  };
  for (const details of content.querySelectorAll<HTMLDetailsElement>('details')) {
    const summary = details.querySelector('summary');
    const body = details.querySelector<HTMLElement>('.disclosure-body');
    if (!summary || !body) continue;
    summary.addEventListener('click', event => {
      event.preventDefault();
      const old = motions.get(details);
      const expanded = !(old?.expanded ?? details.open);
      const from = details.open ? body.getBoundingClientRect().height : 0;
      old?.animation.cancel();
      motions.delete(details);
      details.open = true;
      const natural = body.getBoundingClientRect().height;
      const target = expanded ? natural : 0;
      if (reduced.matches || Math.abs(from - target) < 1) {
        details.open = expanded;
        delete details.dataset.expanded;
        summary.removeAttribute('aria-expanded');
        body.style.overflow = '';
        reportTarget(finalHeight());
        return;
      }
      details.dataset.expanded = String(expanded);
      summary.setAttribute('aria-expanded', String(expanded));
      body.style.overflow = 'hidden';
      const animation = body.animate(
        [{ height: `${from}px` }, { height: `${target}px` }],
        { duration: 160, easing: 'cubic-bezier(.2,.7,.2,1)', fill: 'both' },
      );
      motions.set(details, { animation, body, target, expanded });
      // Size the native viewport to the end state before expanding, after collapsing.
      if (expanded) reportTarget(finalHeight());
      animation.finished.then(() => {
        if (motions.get(details)?.animation !== animation) return;
        motions.delete(details);
        details.open = expanded;
        animation.cancel();
        body.style.overflow = '';
        delete details.dataset.expanded;
        summary.removeAttribute('aria-expanded');
        reportTarget(finalHeight());
      }).catch(() => { /* Superseded by a newer click; preserve its intent. */ });
    });
  }
  return { isAnimating: () => motions.size > 0 };
}
