// First focusable element on every authenticated page. Invisible until
// focused; on focus, renders a visible skip-link that jumps the caret
// past the sidebar into the main content region.
//
// WAI-ARIA APG pattern. Keyboard-only users press Tab once from the page
// landing to reach this link, Enter to skip.

export function SkipToContent() {
  return (
    <a
      href="#main"
      style={{
        position: "absolute",
        left: -9999,
        top: 0,
        padding: "8px 12px",
        background: "var(--color-accent)",
        color: "#fff",
        textDecoration: "none",
        borderRadius: 4,
        zIndex: 9999,
      }}
      onFocus={(e) => {
        e.currentTarget.style.left = "8px";
        e.currentTarget.style.top = "8px";
      }}
      onBlur={(e) => {
        e.currentTarget.style.left = "-9999px";
      }}
    >
      Skip to main content
    </a>
  );
}
