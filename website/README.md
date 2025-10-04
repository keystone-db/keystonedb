# KeystoneDB Website

Modern static website for KeystoneDB built with Astro, React, and TailwindCSS.

## 🚀 Features

- **Lightning Fast**: Built with Astro for optimal performance
- **Fully Responsive**: Mobile-first design that works on all devices
- **Dark Mode**: System-aware dark mode with manual toggle
- **Modern Stack**: Astro 4.x + React + TailwindCSS
- **SEO Optimized**: Meta tags, Open Graph, and semantic HTML
- **Accessible**: WCAG AA compliant

## 📁 Structure

```
website/
├── src/
│   ├── pages/           # Route pages
│   │   ├── index.astro  # Landing page
│   │   ├── docs.astro   # Documentation index
│   │   ├── api.astro    # API reference
│   │   ├── examples.astro
│   │   ├── download.astro
│   │   └── community.astro
│   ├── components/      # React and Astro components
│   │   ├── Header.astro
│   │   ├── Footer.astro
│   │   ├── Hero.astro
│   │   ├── FeatureCard.astro
│   │   └── ThemeToggle.tsx
│   ├── layouts/         # Page layouts
│   │   └── BaseLayout.astro
│   └── styles/          # Global styles
│       └── global.css
├── public/              # Static assets
│   ├── images/
│   └── logos/
└── astro.config.mjs     # Astro configuration
```

## 🧞 Commands

All commands are run from the root of the website directory:

| Command                   | Action                                           |
| :------------------------ | :----------------------------------------------- |
| `npm install`             | Installs dependencies                            |
| `npm run dev`             | Starts local dev server at `localhost:4321`      |
| `npm run build`           | Build your production site to `./dist/`          |
| `npm run preview`         | Preview your build locally, before deploying     |
| `npm run astro ...`       | Run CLI commands like `astro add`, `astro check` |

## 🎨 Design System

### Colors

- **Primary**: Blue (`#0ea5e9`) - Main brand color
- **Accent**: Purple (`#d946ef`) - Accent and highlights
- **Slate**: Gray scale for text and backgrounds

### Typography

- **Sans**: Inter (headings and body text)
- **Mono**: Fira Code (code blocks and inline code)

### Components

- **Buttons**: `.btn`, `.btn-primary`, `.btn-secondary`, `.btn-outline`
- **Cards**: `.card`
- **Navigation**: `.nav-link`

## 📦 Dependencies

### Core
- **astro**: Static site generator
- **@astrojs/react**: React integration
- **@astrojs/tailwind**: TailwindCSS integration
- **react** & **react-dom**: UI components

### UI
- **tailwindcss**: Utility-first CSS
- **lucide-react**: Icon library
- **shiki**: Syntax highlighting

### Dev
- **pagefind**: Static search (to be integrated)

## 🌐 Deployment

### GitHub Pages

```bash
# Build the site
npm run build

# Deploy to GitHub Pages (uses gh-pages branch)
# Configure in repository settings
```

### Vercel

1. Connect your GitHub repository to Vercel
2. Configure build settings:
   - **Build Command**: `npm run build`
   - **Output Directory**: `dist`
   - **Install Command**: `npm install`

### Netlify

1. Connect your GitHub repository to Netlify
2. Configure build settings:
   - **Build Command**: `npm run build`
   - **Publish Directory**: `dist`

## 📝 Content Management

### Adding Documentation Chapters

The book chapters from `../book/` should be copied to `src/content/docs/` during build:

```bash
# Copy book chapters (run from website/ directory)
cp -r ../book/part-*/chapter-*.md src/content/docs/
cp -r ../book/appendices/appendix-*.md src/content/docs/
```

### Adding Examples

Edit `src/pages/examples.astro` to add new examples to the showcase.

## 🔧 Development

### Running Locally

```bash
# Install dependencies
npm install

# Start dev server
npm run dev

# Open browser to http://localhost:4321
```

### Building for Production

```bash
# Build static site
npm run build

# Preview production build
npm run preview
```

## 🎯 Performance

Target metrics:
- **Lighthouse Score**: 95+ on all categories
- **First Contentful Paint**: < 1.5s
- **Time to Interactive**: < 3.0s
- **Total Bundle Size**: < 500KB

Current build output is highly optimized with:
- Automatic code splitting
- Image optimization
- CSS minification
- JS minification
- HTML minification

## 🤝 Contributing

Contributions to the website are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Test locally with `npm run dev`
5. Build with `npm run build` to verify
6. Submit a pull request

## 📄 License

MIT OR Apache-2.0 (same as KeystoneDB)
