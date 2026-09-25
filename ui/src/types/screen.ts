/** The post-wizard main screens (nav bar + `App`'s screen-dispatch
 * switch). Pulled out of `App.tsx` so other modules -- the guided
 * Test Lab walkthroughs' cross-screen banner, in particular -- can
 * navigate the app without importing `App.tsx` itself. */
export type Screen =
  | "overview"
  | "dashboard"
  | "wallet"
  | "inscribe"
  | "explorer"
  | "console"
  | "scripts"
  | "testLab";
