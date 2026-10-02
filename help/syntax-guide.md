# Markdown Syntax & Typora Features Guide

This guide details the Markdown syntax and advanced rendering capabilities supported in **Typora Reader**.

---

## 1. Typography & Themes

Select themes via the dropdown in the top navigation bar:
- **GitHub**: Classic Typora default light theme with crisp borders.
- **Night**: Dark mode with soft slate accents designed for eye comfort.
- **Newsprint**: Literary warm theme featuring Georgia serif typography and paper tones.
- **Gothic**: High-contrast modern dark mode.

### Reading Widths:
Choose between **Narrow (680px)**, **Normal (860px)**, **Wide (1120px)**, or **Full Width** depending on your screen layout.

---

## 2. Text Formatting

| Syntax | Description | Example |
| :--- | :--- | :--- |
| `**bold**` or `__bold__` | Bold weight | **Bold text** |
| `*italic*` or `_italic_` | Italic styling | *Italic text* |
| `~~strikethrough~~` | Strikethrough | ~~Deleted text~~ |
| `==highlight==` | Yellow text highlight | ==Marked text== |
| `` `inline code` `` | Monospace code snippet | `const answer = 42;` |
| `<kbd>Ctrl</kbd>` | Styled keyboard key badge | <kbd>Ctrl</kbd> + <kbd>C</kbd> |

---

## 3. Mathematical Typesetting (KaTeX)

Typora Reader includes offline KaTeX math rendering.

### Inline Math:
Surround expressions with `$ ... $`:
- `$E = mc^2$` produces $E = mc^2$
- `$e^{i\pi} + 1 = 0$` produces $e^{i\pi} + 1 = 0$
- Note: Currency symbols like `$1,500` or `$50` are automatically detected and preserved without breaking math formulas!

### Block / Display Equations:
Surround equations with `$$ ... $$` on separate lines:

```latex
$$\int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}$$
```

Produces:
$$\int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}$$

Multi-line alignment using `\begin{aligned}`:
```latex
$$\begin{aligned}
f(x) &= (x + a)(x + b) \\
     &= x^2 + (a + b)x + ab
\end{aligned}$$
```

---

## 4. Code Blocks with Syntax Highlighting

Fenced code blocks with language identifiers will automatically be highlighted using `highlight.js`:

````markdown
```typescript
interface UserProfile {
  id: string;
  name: string;
  active: boolean;
}
```
````

Hovering over any code block reveals the **Copy** button in the upper right corner.

---

## 5. Rich Tables

GitHub Flavored Markdown (GFM) tables are styled with clean borders and alternating row backgrounds:

```markdown
| Method | Endpoint | Description |
| :--- | :--- | :--- |
| `GET` | `/api/status` | Returns server health and active file |
| `GET` | `/api/file` | Returns file contents and mtime |
| `GET` | `/api/watch` | Checks file modification timestamp |
```

---

## 6. Diagrams (Mermaid)

Create flowcharts, sequence diagrams, and architecture maps using ````mermaid```` blocks:

````markdown
```mermaid
graph LR
    A[Markdown File] --> B[Typora Reader]
    B --> C[Instant Live Preview]
```
````
