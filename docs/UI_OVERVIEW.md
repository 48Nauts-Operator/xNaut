# XNAUT UI Overview

## Application Layout

```
┌────────────────────────────────────────────────────────────────────┐
│ ⚡ XNAUT    [●] Ready    [Anthropic ▼] [Claude 3.5 ▼]   🔗🔔🔐📓⚙️💬 │  ← Top Bar
├────────────────────────────────────────────────────────────────────┤
│ [Terminal 1] [Terminal 2 ×]                            [➕] [▼]    │  ← Tab Bar
├──────────────────────────────────────────────────┬─────────────────┤
│                                                  │ 💬 AI Assistant │
│  $ █                                             ├─────────────────┤
│                                                  │                 │
│  Terminal Output Here                            │  Chat Messages  │
│  with xterm.js rendering                         │  appear here    │
│                                                  │                 │
│                                                  │                 │
│                                                  ├─────────────────┤
│                                                  │ [Ask AI...    ] │
│                                                  │ [Send]          │
└──────────────────────────────────────────────────┴─────────────────┘
```

## Top Bar Components

```
┌──────────────────────────────────────────────────────────────────┐
│ Left Side        │       Center        │      Right Side         │
├──────────────────┼─────────────────────┼────────────────────────┤
│ ⚡ XNAUT         │ [Anthropic ▼]      │ 🔗 Share Session       │
│ [●] Ready        │ [Claude 3.5 ▼]     │ 🔔 Triggers            │
│                  │                     │ 🔐 SSH                 │
│                  │                     │ 📓 Workflows           │
│                  │                     │ ⚙️ Settings            │
│                  │                     │ 💬 Toggle Chat         │
└──────────────────┴─────────────────────┴────────────────────────┘
```

### Status Indicator
- **Green Dot (●)**: Connected/Ready
- **Red Dot (●)**: Disconnected/Error
- **Text**: Current status message

### LLM Selector
- **Provider Dropdown**: Anthropic, OpenAI, OpenRouter, Perplexity
- **Model Dropdown**: Changes based on selected provider

### Action Buttons
1. **🔗 Share**: Share current terminal session
2. **🔔 Triggers**: Manage notification triggers
3. **🔐 SSH**: Manage SSH connections
4. **📓 Workflows**: Manage command workflows
5. **⚙️ Settings**: Configure API keys and preferences
6. **💬 Chat**: Toggle AI assistant panel

## Tab Bar

```
┌─────────────────────────────────────────────────────────┐
│ [Terminal 1] [Terminal 2 ×] [SSH: prod ×]    [➕] [▼]  │
│    active        inactive      ssh tab                  │
└─────────────────────────────────────────────────────────┘
```

- **Active Tab**: Highlighted with accent color
- **Close Button (×)**: Click to close tab
- **New Tab (➕)**: Create new terminal
- **Options (▼)**: Terminal options dropdown

## Terminal Area

```
┌─────────────────────────────────────────────────────┐
│                                                     │
│  ██╗  ██╗███╗   ██╗ █████╗ ██╗   ██╗████████╗     │
│  ╚██╗██╔╝████╗  ██║██╔══██╗██║   ██║╚══██╔══╝     │
│   ╚███╔╝ ██╔██╗ ██║███████║██║   ██║   ██║        │
│   ██╔██╗ ██║╚██╗██║██╔══██║██║   ██║   ██║        │
│  ██╔╝ ██╗██║ ╚████║██║  ██║╚██████╔╝   ██║        │
│  ╚═╝  ╚═╝╚═╝  ╚═══╝╚═╝  ╚═╝ ╚═════╝    ╚═╝        │
│                                                     │
│  ⚡ AI-Powered Native Terminal ⚡                   │
│                                                     │
│  💬 Chat with AI  │  🔐 SSH Sessions  │  📓 Workflows│
│                                                     │
│  Initializing terminal session...                  │
│                                                     │
│  user@host:~$ █                                    │
│                                                     │
└─────────────────────────────────────────────────────┘
```

### Terminal Features
- **ASCII Banner**: Cool XNAUT branding on startup
- **Live Cursor**: Blinking green cursor
- **Color Support**: Full ANSI color support
- **Scrollback**: 10,000 lines of history
- **Selection**: Text selection and copy/paste

## AI Chat Panel (Right Side)

```
┌─────────────────────────────┐
│ 💬 AI Assistant             │
│ [🔍 Analyze] [Clear] [▶]    │
├─────────────────────────────┤
│                             │
│ ┌─────────────────────┐     │
│ │ User message here   │     │
│ │                 [📋]│     │
│ └─────────────────────┘     │
│                             │
│ ┌─────────────────────┐     │
│ │ AI response appears │     │
│ │ here with markdown  │     │
│ │ support         [📋]│     │
│ └─────────────────────┘     │
│                             │
├─────────────────────────────┤
│ [Ask AI anything...       ] │
│ [AI can see your terminal ] │
│ [output!                  ] │
│                      [Send] │
└─────────────────────────────┘
```

### Chat Features
- **Auto-scroll**: Scrolls to latest message
- **Copy Button**: Copy message to clipboard (📋)
- **Context-aware**: AI sees recent terminal output
- **Markdown**: Code blocks, formatting supported
- **Multi-line**: Shift+Enter for new line, Enter to send

## Modals

### Settings Modal

```
┌──────────────────────────────────────┐
│ ⚙️ Settings                       × │
├──────────────────────────────────────┤
│ API Keys                             │
│                                      │
│ Anthropic API Key:                   │
│ [sk-ant-***************************]│
│                                      │
│ OpenAI API Key:                      │
│ [sk-*******************************]│
│                                      │
│ OpenRouter API Key:                  │
│ [sk-or-****************************]│
│                                      │
│ Perplexity API Key:                  │
│ [pplx-*****************************]│
│                                      │
│ ─────────────────────────────────── │
│                                      │
│ Terminal                             │
│                                      │
│ Font Size: [14    ]                  │
│ Theme: [Dark ▼]                      │
│                                      │
├──────────────────────────────────────┤
│                           [Save]     │
└──────────────────────────────────────┘
```

### SSH Profiles Modal

```
┌────────────────────────────────────────────────┐
│ 🔐 SSH Remote Connections               ×     │
├────────────────────────────────────────────────┤
│ [+ New Profile]  [🔍 Search profiles...]       │
├────────────────────────────────────────────────┤
│                                                │
│ ┌──────────────────────────────────────────┐  │
│ │ [●] Production Server                    │  │
│ │     ubuntu@example.com:22         [Edit] │  │
│ │                            [Connect] [×] │  │
│ └──────────────────────────────────────────┘  │
│                                                │
│ ┌──────────────────────────────────────────┐  │
│ │ [ ] Development Server                   │  │
│ │     dev@192.168.1.100:22          [Edit] │  │
│ │                      [Disconnect] [Edit] │  │
│ └──────────────────────────────────────────┘  │
│                                                │
└────────────────────────────────────────────────┘
```

### Workflows Modal

```
┌────────────────────────────────────────────────┐
│ 📓 Workflows & Notebooks                 ×    │
├────────────────────────────────────────────────┤
│ [+ New] [🔴 Record] [🔍 Search...]             │
├────────────────────────────────────────────────┤
│                                                │
│ ┌──────────────────────────────────────────┐  │
│ │ Deploy to Production                     │  │
│ │ Build and deploy app to server           │  │
│ │ 📝 5 commands                            │  │
│ │              [▶ Run] [Edit] [Delete]     │  │
│ └──────────────────────────────────────────┘  │
│                                                │
│ ┌──────────────────────────────────────────┐  │
│ │ Docker Cleanup                           │  │
│ │ Remove unused containers and images      │  │
│ │ 📝 3 commands                            │  │
│ │              [▶ Run] [Edit] [Delete]     │  │
│ └──────────────────────────────────────────┘  │
│                                                │
└────────────────────────────────────────────────┘
```

### Triggers Modal

```
┌────────────────────────────────────────────────┐
│ 🔔 Triggers & Notifications              ×    │
├────────────────────────────────────────────────┤
│ [+ New] [🔔 Test] [🔍 Search...]               │
├────────────────────────────────────────────────┤
│                                                │
│ ┌──────────────────────────────────────────┐  │
│ │ Error Alert                              │  │
│ │ Pattern: error|failed (regex)            │  │
│ │ Message: Error detected!                 │  │
│ │                [Disable] [Edit] [Delete] │  │
│ └──────────────────────────────────────────┘  │
│                                                │
│ ┌──────────────────────────────────────────┐  │
│ │ Build Complete                           │  │
│ │ Pattern: Build succeeded (keyword)       │  │
│ │ Message: Build finished successfully     │  │
│ │                 [Enable] [Edit] [Delete] │  │
│ └──────────────────────────────────────────┘  │
│                                                │
└────────────────────────────────────────────────┘
```

### Command History Modal (Ctrl+R)

```
┌────────────────────────────────────────────────┐
│ 🔍 Command History (Ctrl+R)              ×    │
├────────────────────────────────────────────────┤
│ [Search command history...                   ] │
├────────────────────────────────────────────────┤
│                                                │
│ ┌──────────────────────────────────────────┐  │
│ │ 2025-10-06 14:32:15                      │  │
│ │ docker ps -a                             │  │
│ └──────────────────────────────────────────┘  │
│                                                │
│ ┌──────────────────────────────────────────┐  │
│ │ 2025-10-06 14:30:42                      │  │
│ │ git status                               │  │
│ └──────────────────────────────────────────┘  │
│                                                │
│ ┌──────────────────────────────────────────┐  │
│ │ 2025-10-06 14:25:18                      │  │
│ │ npm install                              │  │
│ └──────────────────────────────────────────┘  │
│                                                │
└────────────────────────────────────────────────┘
```

### Share Session Modal

```
┌────────────────────────────────────────────────┐
│ 🔗 Share Session                         ×    │
├────────────────────────────────────────────────┤
│                                                │
│ Share Code:                                    │
│ ┌──────────────────────────────────────────┐  │
│ │          A B C 1 2 3                     │  │
│ └──────────────────────────────────────────┘  │
│                                                │
│ ℹ️ Share this code with others so they can   │
│    join your terminal session.                │
│                                                │
├────────────────────────────────────────────────┤
│                    [Copy to Clipboard]         │
└────────────────────────────────────────────────┘
```

## Color Scheme

### Dark Theme (Default)
```css
Background Primary:   #0a0a0f  (Very dark blue-black)
Background Secondary: #1a1a1f  (Dark charcoal)
Background Tertiary:  #2a2a2f  (Lighter charcoal)
Text Primary:         #e0e0e0  (Light gray)
Text Secondary:       #a0a0a0  (Medium gray)
Accent:              #3b82f6  (Blue)
Accent Hover:        #2563eb  (Darker blue)
Success:             #10b981  (Green)
Danger:              #ef4444  (Red)
Warning:             #f59e0b  (Orange)
Border:              #2a2a2f  (Charcoal)
```

### Terminal Colors
```css
Black:         #282c34
Red:           #ff6b6b
Green:         #51cf66
Yellow:        #ffd93d
Blue:          #6bcfff
Magenta:       #ff6ac1
Cyan:          #4adfdf
White:         #abb2bf

Cursor:        #00ff00 (Green, blinking)
Background:    #0a0a0a (Pure black)
```

## Interactions

### Hover Effects
- **Buttons**: Background changes to accent color
- **List Items**: Slight slide to right (transform: translateX(4px))
- **Tabs**: Background lightens on hover
- **Modal Overlay**: Semi-transparent dark background

### Transitions
- **All buttons**: 0.2s ease
- **Modals**: Fade in/out
- **Notifications**: Slide up from bottom
- **Chat messages**: Fade in

### Animations
- **Cursor**: Blinking animation
- **Status dot**: Pulse animation when connected
- **Recording indicator**: Pulse animation (red)
- **Loading states**: Spinner rotation

## Responsive Behavior

### Window Resize
- Terminal auto-resizes to fit container
- Chat panel maintains fixed width (280px)
- Modals stay centered
- Scrollbars appear when content overflows

### Minimum Sizes
- Window: 800x600px (recommended minimum)
- Chat panel: 280px width (collapsible)
- Terminal: Adapts to available space

## Accessibility

### Keyboard Navigation
- **Tab**: Navigate between focusable elements
- **Enter**: Activate buttons/submit forms
- **Escape**: Close modals
- **Ctrl+R**: Open command history
- **Ctrl+T**: New terminal tab
- **Ctrl+W**: Close current tab

### Screen Reader Support
- All buttons have aria-labels
- Modals have proper aria-roles
- Form inputs have associated labels

## State Indicators

### Connection Status
- **Green dot + "Ready"**: Normal operation
- **Red dot + "Disconnected"**: No connection
- **Yellow dot + "Connecting..."**: In progress

### Recording Status
- **Normal**: Standard UI
- **Recording**: Red pulsing indicator in workflows

### SSH Connection
- **Green dot**: Connected
- **Gray dot**: Disconnected
- **Pulsing green**: Active connection

---

**Design Philosophy**: Clean, modern, minimal, professional
**Target Users**: Developers, DevOps, System Administrators
**Platform**: Desktop (Tauri native app)
