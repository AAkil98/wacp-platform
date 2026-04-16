import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { useUiStore } from "./ui";

// Default matchMedia mock (prefers light)
function mockMatchMedia(prefersDark = false) {
  return vi.fn().mockReturnValue({
    matches: prefersDark,
    media: "(prefers-color-scheme: dark)",
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
    onchange: null,
  });
}

function resetStore() {
  useUiStore.setState({
    sidebarOpen: true,
    theme: "system",
  });
}

describe("useUiStore", () => {
  let originalMatchMedia: typeof window.matchMedia;

  beforeEach(() => {
    resetStore();
    // Save and install a default matchMedia so applyTheme never crashes
    originalMatchMedia = window.matchMedia;
    window.matchMedia = mockMatchMedia(false);
    // Clean up DOM classes set by applyTheme
    document.documentElement.classList.remove("light", "dark");
  });

  afterEach(() => {
    window.matchMedia = originalMatchMedia;
  });

  // ---- Initial state ----
  describe("initial state", () => {
    it("has sidebarOpen true", () => {
      expect(useUiStore.getState().sidebarOpen).toBe(true);
    });

    it("has theme set to 'system'", () => {
      expect(useUiStore.getState().theme).toBe("system");
    });
  });

  // ---- toggleSidebar ----
  describe("toggleSidebar", () => {
    it("toggles sidebarOpen from true to false", () => {
      expect(useUiStore.getState().sidebarOpen).toBe(true);

      useUiStore.getState().toggleSidebar();

      expect(useUiStore.getState().sidebarOpen).toBe(false);
    });

    it("toggles sidebarOpen from false to true", () => {
      useUiStore.setState({ sidebarOpen: false });

      useUiStore.getState().toggleSidebar();

      expect(useUiStore.getState().sidebarOpen).toBe(true);
    });

    it("double toggle restores original state", () => {
      useUiStore.getState().toggleSidebar();
      useUiStore.getState().toggleSidebar();

      expect(useUiStore.getState().sidebarOpen).toBe(true);
    });

    it("does not affect theme when toggling sidebar", () => {
      useUiStore.setState({ theme: "dark" });

      useUiStore.getState().toggleSidebar();

      expect(useUiStore.getState().theme).toBe("dark");
    });
  });

  // ---- setTheme ----
  describe("setTheme", () => {
    it("sets theme to 'light'", () => {
      useUiStore.getState().setTheme("light");

      expect(useUiStore.getState().theme).toBe("light");
    });

    it("sets theme to 'dark'", () => {
      useUiStore.getState().setTheme("dark");

      expect(useUiStore.getState().theme).toBe("dark");
    });

    it("sets theme to 'system'", () => {
      useUiStore.setState({ theme: "dark" });

      useUiStore.getState().setTheme("system");

      expect(useUiStore.getState().theme).toBe("system");
    });

    it("does not affect sidebarOpen when setting theme", () => {
      useUiStore.setState({ sidebarOpen: false });

      useUiStore.getState().setTheme("dark");

      expect(useUiStore.getState().sidebarOpen).toBe(false);
    });

    // ---- applyTheme DOM effects ----
    describe("applyTheme DOM effects", () => {
      it("adds 'light' class for light theme", () => {
        useUiStore.getState().setTheme("light");

        expect(document.documentElement.classList.contains("light")).toBe(true);
        expect(document.documentElement.classList.contains("dark")).toBe(false);
      });

      it("adds 'dark' class for dark theme", () => {
        useUiStore.getState().setTheme("dark");

        expect(document.documentElement.classList.contains("dark")).toBe(true);
        expect(document.documentElement.classList.contains("light")).toBe(false);
      });

      it("removes previous theme class when switching", () => {
        useUiStore.getState().setTheme("dark");
        expect(document.documentElement.classList.contains("dark")).toBe(true);

        useUiStore.getState().setTheme("light");
        expect(document.documentElement.classList.contains("light")).toBe(true);
        expect(document.documentElement.classList.contains("dark")).toBe(false);
      });

      it("applies system preference for 'system' theme (prefers dark)", () => {
        window.matchMedia = mockMatchMedia(true);

        useUiStore.getState().setTheme("system");

        expect(document.documentElement.classList.contains("dark")).toBe(true);
        expect(document.documentElement.classList.contains("light")).toBe(false);
      });

      it("applies system preference for 'system' theme (prefers light)", () => {
        window.matchMedia = mockMatchMedia(false);

        useUiStore.getState().setTheme("system");

        expect(document.documentElement.classList.contains("light")).toBe(true);
        expect(document.documentElement.classList.contains("dark")).toBe(false);
      });
    });
  });

  // ---- Edge cases ----
  describe("edge cases", () => {
    it("setting the same theme twice is idempotent", () => {
      useUiStore.getState().setTheme("dark");
      useUiStore.getState().setTheme("dark");

      expect(useUiStore.getState().theme).toBe("dark");
      expect(document.documentElement.classList.contains("dark")).toBe(true);
    });

    it("rapid toggles maintain correct state", () => {
      for (let i = 0; i < 10; i++) {
        useUiStore.getState().toggleSidebar();
      }
      // 10 toggles from true => should be back to true
      expect(useUiStore.getState().sidebarOpen).toBe(true);
    });

    it("rapid toggles with odd count invert state", () => {
      for (let i = 0; i < 7; i++) {
        useUiStore.getState().toggleSidebar();
      }
      // 7 toggles from true => false
      expect(useUiStore.getState().sidebarOpen).toBe(false);
    });
  });
});
