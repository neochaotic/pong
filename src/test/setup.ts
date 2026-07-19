import "@testing-library/jest-dom/vitest";

// jsdom has no Web Animations API, which Svelte 5's transitions (fade, fly,
// slide...) rely on. Without this stub, any component using a transition
// throws "element.animate is not a function" as soon as it mounts in a test.
if (!Element.prototype.animate) {
  Element.prototype.animate = function () {
    return {
      finished: Promise.resolve(),
      cancel: () => {},
      pause: () => {},
      play: () => {},
      reverse: () => {},
      finish: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      onfinish: null,
      oncancel: null,
      currentTime: 0,
      playState: "finished",
    } as unknown as Animation;
  };
}
