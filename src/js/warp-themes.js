// Warp Terminal themes converted to xNAUT format
// Source: https://github.com/warpdotdev/themes/tree/main/standard
const WARP_THEMES = {
  'Cyberpunk Neon': {
    bg: '#000b1e', fg: '#0abdc6', cursor: '#0abdc6', chrome: '#000814', selection: 'rgba(255,255,255,0.2)',
    black: '#123e7c', red: '#ff0000', green: '#d300c4', yellow: '#f57800', blue: '#123e7c', magenta: '#711c91', cyan: '#0abdc6', white: '#d7d7d5',
    brightBlack: '#1c61c2', brightRed: '#ff0000', brightGreen: '#d300c4', brightYellow: '#f57800', brightBlue: '#00ff00', brightMagenta: '#711c91', brightCyan: '#0abdc6', brightWhite: '#d7d7d5',
  },
  'Night Owl': {
    bg: '#011627', fg: '#d6deeb', cursor: '#7e57c2', chrome: '#010f1e', selection: 'rgba(255,255,255,0.2)',
    black: '#011627', red: '#EF5350', green: '#22da6e', yellow: '#addb67', blue: '#82aaff', magenta: '#c792ea', cyan: '#21c7a8', white: '#ffffff',
    brightBlack: '#575656', brightRed: '#ef5350', brightGreen: '#22da6e', brightYellow: '#ffeb95', brightBlue: '#82aaff', brightMagenta: '#c792ea', brightCyan: '#7fdbca', brightWhite: '#ffffff',
  },
  'Synthwave 84': {
    bg: '#191621', fg: '#f1f1f1', cursor: '#f92aad', chrome: '#14111c', selection: 'rgba(255,255,255,0.2)',
    black: '#382C4D', red: '#fe4450', green: '#72f1b8', yellow: '#fede5d', blue: '#34d3fb', magenta: '#f92aad', cyan: '#36f9f6', white: '#f2ebe0',
    brightBlack: '#7b6a98', brightRed: '#ff7d86', brightGreen: '#c6ffe5', brightYellow: '#fff0b4', brightBlue: '#aaeeff', brightMagenta: '#ffa8df', brightCyan: '#9efffd', brightWhite: '#f1f1f1',
  },
  'Material Theme': {
    bg: '#1e282d', fg: '#c4c7d1', cursor: '#80cbc4', chrome: '#182125', selection: 'rgba(255,255,255,0.2)',
    black: '#666666', red: '#eb606b', green: '#c3e88d', yellow: '#f7eb95', blue: '#80cbc4', magenta: '#ff2f90', cyan: '#aeddff', white: '#ffffff',
    brightBlack: '#ff262b', brightRed: '#eb606b', brightGreen: '#c3e88d', brightYellow: '#f7eb95', brightBlue: '#7dc6bf', brightMagenta: '#6c71c4', brightCyan: '#35434d', brightWhite: '#ffffff',
  },
  'Everforest Hard': {
    bg: '#2b3339', fg: '#d3c6aa', cursor: '#7a8478', chrome: '#232a2f', selection: 'rgba(255,255,255,0.2)',
    black: '#445055', red: '#e67e80', green: '#a7c080', yellow: '#dbbc7f', blue: '#7fbbb3', magenta: '#d699b6', cyan: '#83c092', white: '#d3c6aa',
    brightBlack: '#445055', brightRed: '#e67e80', brightGreen: '#a7c080', brightYellow: '#dbbc7f', brightBlue: '#7fbbb3', brightMagenta: '#d699b6', brightCyan: '#83c092', brightWhite: '#d3c6aa',
  },
  'Challenger Deep': {
    bg: '#1e1c31', fg: '#cbe1e7', cursor: '#65b2ff', chrome: '#181630', selection: 'rgba(255,255,255,0.2)',
    black: '#141228', red: '#ff5458', green: '#62d196', yellow: '#ffb378', blue: '#65b2ff', magenta: '#906cff', cyan: '#63f2f1', white: '#a6b3cc',
    brightBlack: '#565575', brightRed: '#ff8080', brightGreen: '#95ffa4', brightYellow: '#ffe9aa', brightBlue: '#91ddff', brightMagenta: '#c991e1', brightCyan: '#aaffe4', brightWhite: '#cbe3e7',
  },
  'Poimandres': {
    bg: '#1B1E28', fg: '#ACCDFF', cursor: '#C5E9FF', chrome: '#161821', selection: 'rgba(255,255,255,0.2)',
    black: '#1B1E28', red: '#679DFF', green: '#E4C7FF', yellow: '#FAC2FF', blue: '#DDFFFF', magenta: '#C5E9FF', cyan: '#DDFFFF', white: '#FFFFFF',
    brightBlack: '#ACCDFF', brightRed: '#679DFF', brightGreen: '#E4C7FF', brightYellow: '#FAC2FF', brightBlue: '#D7FFFF', brightMagenta: '#C5E9FF', brightCyan: '#D7FFFF', brightWhite: '#FFFFFF',
  },
  'Tokyo Night Storm': {
    bg: '#24283b', fg: '#a9b1d6', cursor: '#7aa2f7', chrome: '#1e2233', selection: 'rgba(255,255,255,0.2)',
    black: '#32344a', red: '#f7768e', green: '#9ece6a', yellow: '#e0af68', blue: '#7aa2f7', magenta: '#ad8ee6', cyan: '#449dab', white: '#9699a8',
    brightBlack: '#444b6a', brightRed: '#ff7a93', brightGreen: '#b9f27c', brightYellow: '#ff9e64', brightBlue: '#7da6ff', brightMagenta: '#bb9af7', brightCyan: '#0db9d7', brightWhite: '#acb0d0',
  },
  'Halcyon': {
    bg: '#1d2433', fg: '#a2aabc', cursor: '#8695b7', chrome: '#181e2b', selection: 'rgba(255,255,255,0.2)',
    black: '#8695b7', red: '#f07078', green: '#bae67e', yellow: '#ffd580', blue: '#5ccfe6', magenta: '#c3a6ff', cyan: '#5ccfe6', white: '#d7dce2',
    brightBlack: '#171c28', brightRed: '#ef6b73', brightGreen: '#bae67e', brightYellow: '#ffd580', brightBlue: '#5ccfe6', brightMagenta: '#c3a6ff', brightCyan: '#5ccfe6', brightWhite: '#d7dce2',
  },
  'Outrun': {
    bg: '#0c0a20', fg: '#7984D1', cursor: '#fc28a8', chrome: '#080618', selection: 'rgba(255,255,255,0.2)',
    black: '#283034', red: '#ff0081', green: '#a7da1e', yellow: '#f7b83d', blue: '#1ea8fc', magenta: '#A875FF', cyan: '#16f1fc', white: '#f9faff',
    brightBlack: '#435056', brightRed: '#ff2e97', brightGreen: '#cbfc44', brightYellow: '#ffd400', brightBlue: '#42c6ff', brightMagenta: '#ff2afc', brightCyan: '#39fff6', brightWhite: '#ffffff',
  },
  'Shades of Purple': {
    bg: '#2d2b55', fg: '#ffffff', cursor: '#fad000', chrome: '#252348', selection: 'rgba(255,255,255,0.2)',
    black: '#000000', red: '#ec3a37', green: '#3ad900', yellow: '#fad000', blue: '#7857fe', magenta: '#ff2c70', cyan: '#80fcff', white: '#ffffff',
    brightBlack: '#5c5c61', brightRed: '#ec3a37', brightGreen: '#3ad900', brightYellow: '#fad000', brightBlue: '#6943ff', brightMagenta: '#fb94ff', brightCyan: '#80fcff', brightWhite: '#ffffff',
  },
  'Spaceduck': {
    bg: '#0f111b', fg: '#ecf0c1', cursor: '#b3a1e6', chrome: '#0a0c15', selection: 'rgba(255,255,255,0.2)',
    black: '#000000', red: '#e33400', green: '#5ccc96', yellow: '#b3a1e6', blue: '#00a3cc', magenta: '#f2ce00', cyan: '#7a5ccc', white: '#686f9a',
    brightBlack: '#686f9a', brightRed: '#e33400', brightGreen: '#5ccc96', brightYellow: '#b3a1e6', brightBlue: '#00a3cc', brightMagenta: '#f2ce00', brightCyan: '#7a5ccc', brightWhite: '#f0f1ce',
  },
  'Cobalt 2': {
    bg: '#122637', fg: '#ffffff', cursor: '#1460d2', chrome: '#0e1e2d', selection: 'rgba(255,255,255,0.2)',
    black: '#000000', red: '#ff0000', green: '#37dd21', yellow: '#fee409', blue: '#1460d2', magenta: '#ff005d', cyan: '#00bbbb', white: '#bbbbbb',
    brightBlack: '#545454', brightRed: '#f40d17', brightGreen: '#3bcf1d', brightYellow: '#ecc809', brightBlue: '#5555ff', brightMagenta: '#ff55ff', brightCyan: '#6ae3f9', brightWhite: '#ffffff',
  },
  'GitHub Dark': {
    bg: '#0d1117', fg: '#c9d1d9', cursor: '#F78166', chrome: '#090e14', selection: 'rgba(255,255,255,0.2)',
    black: '#0d1117', red: '#ff7b72', green: '#3fb950', yellow: '#d29922', blue: '#58a6ff', magenta: '#bc8cff', cyan: '#76e3ea', white: '#b1bac4',
    brightBlack: '#161b22', brightRed: '#ffa198', brightGreen: '#56d364', brightYellow: '#e3b341', brightBlue: '#79c0ff', brightMagenta: '#d2a8ff', brightCyan: '#b3f0ff', brightWhite: '#b1bac4',
  },
  'Lucario': {
    bg: '#2B3E50', fg: '#F8F8F2', cursor: '#F8F8F2', chrome: '#233241', selection: 'rgba(255,255,255,0.2)',
    black: '#4F4F4F', red: '#FF6C60', green: '#FBB036', yellow: '#FFFFB6', blue: '#5796ED', magenta: '#FF73FD', cyan: '#8EE478', white: '#EEEEEE',
    brightBlack: '#4F4F4F', brightRed: '#FA6960', brightGreen: '#FBB036', brightYellow: '#FEFFB9', brightBlue: '#6B9FED', brightMagenta: '#FC6FFA', brightCyan: '#8EE478', brightWhite: '#FFFFFF',
  },
  'Matrix Dracula': {
    bg: '#282a36', fg: '#00ff51', cursor: '#00c2ff', chrome: '#21232e', selection: 'rgba(255,255,255,0.2)',
    black: '#000000', red: '#c91b00', green: '#00c200', yellow: '#c7c400', blue: '#3650c2', magenta: '#c930c7', cyan: '#00c5c7', white: '#c7c7c7',
    brightBlack: '#676767', brightRed: '#ff6d67', brightGreen: '#00f74e', brightYellow: '#fefb67', brightBlue: '#6871ff', brightMagenta: '#ff76ff', brightCyan: '#5ffdff', brightWhite: '#fffefe',
  },
  'Jellybeans': {
    bg: '#121212', fg: '#dedede', cursor: '#e1c0fa', chrome: '#0d0d0d', selection: 'rgba(255,255,255,0.2)',
    black: '#929292', red: '#e27373', green: '#94b979', yellow: '#ffba7b', blue: '#97bedc', magenta: '#e1c0fa', cyan: '#00988e', white: '#dedede',
    brightBlack: '#929292', brightRed: '#ffa1a1', brightGreen: '#94b979', brightYellow: '#ffdca0', brightBlue: '#97bedc', brightMagenta: '#e1c0fa', brightCyan: '#00988e', brightWhite: '#ffffff',
  },
  'Panda Syntax': {
    bg: '#25282a', fg: '#f3f2f2', cursor: '#65bcfe', chrome: '#1e2022', selection: 'rgba(255,255,255,0.2)',
    black: '#29292a', red: '#fe2b6c', green: '#14fbdc', yellow: '#feb76b', blue: '#6db0fe', magenta: '#fe74b4', cyan: '#15fbdb', white: '#f3f2f2',
    brightBlack: '#6f7683', brightRed: '#fe2b6c', brightGreen: '#14fbdc', brightYellow: '#ffc88f', brightBlue: '#65bcfe', brightMagenta: '#fea9d8', brightCyan: '#15fbdb', brightWhite: '#f3f2f2',
  },
  'Soft One Dark': {
    bg: '#282C34', fg: '#ABB2BF', cursor: '#64AEEF', chrome: '#21252b', selection: 'rgba(255,255,255,0.2)',
    black: '#9e9e9e', red: '#ff8272', green: '#97C379', yellow: '#E4C17B', blue: '#64AEEF', magenta: '#C676DD', cyan: '#828996', white: '#ABB2BF',
    brightBlack: '#8e8e8e', brightRed: '#ffc4bd', brightGreen: '#d6fcb9', brightYellow: '#fefdd5', brightBlue: '#c1e3fe', brightMagenta: '#ffb1fe', brightCyan: '#e5e6fe', brightWhite: '#feffff',
  },
  'Horizon Dark': {
    bg: '#1c1e26', fg: '#e0e0e0', cursor: '#26bbd9', chrome: '#171920', selection: 'rgba(255,255,255,0.2)',
    black: '#16161c', red: '#e95678', green: '#29d398', yellow: '#fab795', blue: '#26bbd9', magenta: '#ee64ac', cyan: '#59e1e3', white: '#d5d8da',
    brightBlack: '#5b5858', brightRed: '#ec6a88', brightGreen: '#3fdaa4', brightYellow: '#fbc3a7', brightBlue: '#3fc4de', brightMagenta: '#f075b5', brightCyan: '#6be4e6', brightWhite: '#d5d8da',
  },
  // --- iTerm2-Color-Schemes imports (MIT, github.com/mbadolato/iTerm2-Color-Schemes) ---
  'Catppuccin Mocha': {
    bg: '#1e1e2e', fg: '#cdd6f4', cursor: '#f5e0dc', chrome: '#181825', selection: 'rgba(255,255,255,0.2)',
    black: '#45475a', red: '#f38ba8', green: '#a6e3a1', yellow: '#f9e2af', blue: '#89b4fa', magenta: '#f5c2e7', cyan: '#94e2d5', white: '#bac2de',
    brightBlack: '#585b70', brightRed: '#f7aec2', brightGreen: '#c2ecbf', brightYellow: '#fcd682', brightBlue: '#aeccfc', brightMagenta: '#f398da', brightCyan: '#b1eae1', brightWhite: '#a6adc8',
  },
  'Catppuccin Macchiato': {
    bg: '#24273a', fg: '#cad3f5', cursor: '#f4dbd6', chrome: '#1d1f2e', selection: 'rgba(255,255,255,0.2)',
    black: '#494d64', red: '#ed8796', green: '#a6da95', yellow: '#eed49f', blue: '#8aadf4', magenta: '#f5bde6', cyan: '#8bd5ca', white: '#b8c0e0',
    brightBlack: '#5b6078', brightRed: '#f2a7b2', brightGreen: '#bde3b0', brightYellow: '#f4e3c1', brightBlue: '#adc5f7', brightMagenta: '#f493da', brightCyan: '#a5ded6', brightWhite: '#a5adcb',
  },
  'Catppuccin Frappé': {
    bg: '#303446', fg: '#c6d0f5', cursor: '#f2d5cf', chrome: '#262a38', selection: 'rgba(255,255,255,0.2)',
    black: '#51576d', red: '#e78284', green: '#a6d189', yellow: '#e5c890', blue: '#8caaee', magenta: '#f4b8e4', cyan: '#81c8be', white: '#b5bfe2',
    brightBlack: '#626880', brightRed: '#eda0a2', brightGreen: '#b9dba2', brightYellow: '#ecd7ae', brightBlue: '#adc2f3', brightMagenta: '#f38ed8', brightCyan: '#98d2ca', brightWhite: '#a5adce',
  },
  'Catppuccin Latte': {
    bg: '#eff1f5', fg: '#4c4f69', cursor: '#dc8a78', chrome: '#e5e7eb', selection: 'rgba(0,0,0,0.15)',
    black: '#bcc0cc', red: '#d20f39', green: '#40a02b', yellow: '#df8e1d', blue: '#1e66f5', magenta: '#ea76cb', cyan: '#179299', white: '#5c5f77',
    brightBlack: '#acb0be', brightRed: '#e7103f', brightGreen: '#46b02f', brightYellow: '#e49931', brightBlue: '#3878f6', brightMagenta: '#ef95d7', brightCyan: '#19a1a8', brightWhite: '#6c6f85',
  },
  'Gruvbox Dark': {
    bg: '#282828', fg: '#ebdbb2', cursor: '#ebdbb2', chrome: '#202020', selection: 'rgba(255,255,255,0.2)',
    black: '#282828', red: '#cc241d', green: '#98971a', yellow: '#d79921', blue: '#458588', magenta: '#b16286', cyan: '#689d6a', white: '#a89984',
    brightBlack: '#928374', brightRed: '#fb4934', brightGreen: '#b8bb26', brightYellow: '#fabd2f', brightBlue: '#83a598', brightMagenta: '#d3869b', brightCyan: '#8ec07c', brightWhite: '#ebdbb2',
  },
  'Gruvbox Light': {
    bg: '#fbf1c7', fg: '#3c3836', cursor: '#3c3836', chrome: '#f1e7bf', selection: 'rgba(0,0,0,0.15)',
    black: '#fbf1c7', red: '#cc241d', green: '#98971a', yellow: '#d79921', blue: '#458588', magenta: '#b16286', cyan: '#689d6a', white: '#7c6f64',
    brightBlack: '#928374', brightRed: '#9d0006', brightGreen: '#79740e', brightYellow: '#b57614', brightBlue: '#076678', brightMagenta: '#8f3f71', brightCyan: '#427b58', brightWhite: '#3c3836',
  },
  'Nord': {
    bg: '#2e3440', fg: '#d8dee9', cursor: '#eceff4', chrome: '#252a33', selection: 'rgba(255,255,255,0.2)',
    black: '#3b4252', red: '#bf616a', green: '#a3be8c', yellow: '#ebcb8b', blue: '#81a1c1', magenta: '#b48ead', cyan: '#88c0d0', white: '#e5e9f0',
    brightBlack: '#596377', brightRed: '#bf616a', brightGreen: '#a3be8c', brightYellow: '#ebcb8b', brightBlue: '#81a1c1', brightMagenta: '#b48ead', brightCyan: '#8fbcbb', brightWhite: '#eceff4',
  },
  'Nord Light': {
    bg: '#e5e9f0', fg: '#414858', cursor: '#7bb3c3', chrome: '#dce0e6', selection: 'rgba(0,0,0,0.15)',
    black: '#3b4252', red: '#bf616a', green: '#96b17f', yellow: '#c5a565', blue: '#81a1c1', magenta: '#b48ead', cyan: '#7bb3c3', white: '#a5abb6',
    brightBlack: '#4c566a', brightRed: '#bf616a', brightGreen: '#96b17f', brightYellow: '#c5a565', brightBlue: '#81a1c1', brightMagenta: '#b48ead', brightCyan: '#82afae', brightWhite: '#eceff4',
  },
  'Rosé Pine': {
    bg: '#191724', fg: '#e0def4', cursor: '#e0def4', chrome: '#14121d', selection: 'rgba(255,255,255,0.2)',
    black: '#26233a', red: '#eb6f92', green: '#31748f', yellow: '#f6c177', blue: '#9ccfd8', magenta: '#c4a7e7', cyan: '#ebbcba', white: '#e0def4',
    brightBlack: '#6e6a86', brightRed: '#eb6f92', brightGreen: '#31748f', brightYellow: '#f6c177', brightBlue: '#9ccfd8', brightMagenta: '#c4a7e7', brightCyan: '#ebbcba', brightWhite: '#e0def4',
  },
  'Rosé Pine Moon': {
    bg: '#232136', fg: '#e0def4', cursor: '#e0def4', chrome: '#1c1a2b', selection: 'rgba(255,255,255,0.2)',
    black: '#393552', red: '#eb6f92', green: '#3e8fb0', yellow: '#f6c177', blue: '#9ccfd8', magenta: '#c4a7e7', cyan: '#ea9a97', white: '#e0def4',
    brightBlack: '#6e6a86', brightRed: '#eb6f92', brightGreen: '#3e8fb0', brightYellow: '#f6c177', brightBlue: '#9ccfd8', brightMagenta: '#c4a7e7', brightCyan: '#ea9a97', brightWhite: '#e0def4',
  },
  'Rosé Pine Dawn': {
    bg: '#faf4ed', fg: '#575279', cursor: '#575279', chrome: '#f0eae4', selection: 'rgba(0,0,0,0.15)',
    black: '#f2e9e1', red: '#b4637a', green: '#286983', yellow: '#ea9d34', blue: '#56949f', magenta: '#907aa9', cyan: '#d7827e', white: '#575279',
    brightBlack: '#9893a5', brightRed: '#b4637a', brightGreen: '#286983', brightYellow: '#ea9d34', brightBlue: '#56949f', brightMagenta: '#907aa9', brightCyan: '#d7827e', brightWhite: '#575279',
  },
  'Kanagawa Wave': {
    bg: '#1f1f28', fg: '#dcd7ba', cursor: '#dcd7ba', chrome: '#191920', selection: 'rgba(255,255,255,0.2)',
    black: '#090618', red: '#c34043', green: '#76946a', yellow: '#c0a36e', blue: '#7e9cd8', magenta: '#957fb8', cyan: '#6a9589', white: '#c8c093',
    brightBlack: '#727169', brightRed: '#e82424', brightGreen: '#98bb6c', brightYellow: '#e6c384', brightBlue: '#7fb4ca', brightMagenta: '#938aa9', brightCyan: '#7aa89f', brightWhite: '#dcd7ba',
  },
  'Kanagawa Dragon': {
    bg: '#181616', fg: '#c5c9c5', cursor: '#c8c093', chrome: '#131212', selection: 'rgba(255,255,255,0.2)',
    black: '#0d0c0c', red: '#c4746e', green: '#8a9a7b', yellow: '#c4b28a', blue: '#8ba4b0', magenta: '#a292a3', cyan: '#8ea4a2', white: '#c8c093',
    brightBlack: '#a6a69c', brightRed: '#e46876', brightGreen: '#87a987', brightYellow: '#e6c384', brightBlue: '#7fb4ca', brightMagenta: '#938aa9', brightCyan: '#7aa89f', brightWhite: '#c5c9c5',
  },
  'One Half Dark': {
    bg: '#282c34', fg: '#dcdfe4', cursor: '#a3b3cc', chrome: '#20232a', selection: 'rgba(255,255,255,0.2)',
    black: '#282c34', red: '#e06c75', green: '#98c379', yellow: '#e5c07b', blue: '#61afef', magenta: '#c678dd', cyan: '#56b6c2', white: '#dcdfe4',
    brightBlack: '#5d677a', brightRed: '#e06c75', brightGreen: '#98c379', brightYellow: '#e5c07b', brightBlue: '#61afef', brightMagenta: '#c678dd', brightCyan: '#56b6c2', brightWhite: '#dcdfe4',
  },
  'One Half Light': {
    bg: '#fafafa', fg: '#383a42', cursor: '#a5b4e5', chrome: '#f0f0f0', selection: 'rgba(0,0,0,0.15)',
    black: '#383a42', red: '#e45649', green: '#50a14f', yellow: '#c18401', blue: '#0184bc', magenta: '#a626a4', cyan: '#0997b3', white: '#bababa',
    brightBlack: '#4f525e', brightRed: '#e06c75', brightGreen: '#98c379', brightYellow: '#d8b36e', brightBlue: '#61afef', brightMagenta: '#c678dd', brightCyan: '#56b6c2', brightWhite: '#ffffff',
  },
  'Ayu Dark': {
    bg: '#0b0e14', fg: '#bfbdb6', cursor: '#e6b450', chrome: '#090b10', selection: 'rgba(255,255,255,0.2)',
    black: '#11151c', red: '#ea6c73', green: '#7fd962', yellow: '#f9af4f', blue: '#53bdfa', magenta: '#cda1fa', cyan: '#90e1c6', white: '#c7c7c7',
    brightBlack: '#686868', brightRed: '#f07178', brightGreen: '#aad94c', brightYellow: '#ffb454', brightBlue: '#59c2ff', brightMagenta: '#d2a6ff', brightCyan: '#95e6cb', brightWhite: '#ffffff',
  },
  'Ayu Mirage': {
    bg: '#1f2430', fg: '#cccac2', cursor: '#ffcc66', chrome: '#191d26', selection: 'rgba(255,255,255,0.2)',
    black: '#171b24', red: '#ed8274', green: '#87d96c', yellow: '#facc6e', blue: '#6dcbfa', magenta: '#dabafa', cyan: '#90e1c6', white: '#c7c7c7',
    brightBlack: '#686868', brightRed: '#f28779', brightGreen: '#d5ff80', brightYellow: '#ffd173', brightBlue: '#73d0ff', brightMagenta: '#dfbfff', brightCyan: '#95e6cb', brightWhite: '#ffffff',
  },
  'Tokyo Night': {
    bg: '#1a1b26', fg: '#c0caf5', cursor: '#c0caf5', chrome: '#15161e', selection: 'rgba(255,255,255,0.2)',
    black: '#15161e', red: '#f7768e', green: '#9ece6a', yellow: '#e0af68', blue: '#7aa2f7', magenta: '#bb9af7', cyan: '#7dcfff', white: '#a9b1d6',
    brightBlack: '#414868', brightRed: '#f7768e', brightGreen: '#9ece6a', brightYellow: '#e0af68', brightBlue: '#7aa2f7', brightMagenta: '#bb9af7', brightCyan: '#7dcfff', brightWhite: '#c0caf5',
  },
  'Tokyo Night Moon': {
    bg: '#222436', fg: '#c8d3f5', cursor: '#c8d3f5', chrome: '#1b1d2b', selection: 'rgba(255,255,255,0.2)',
    black: '#1b1d2b', red: '#ff757f', green: '#c3e88d', yellow: '#ffc777', blue: '#82aaff', magenta: '#c099ff', cyan: '#86e1fc', white: '#828bb8',
    brightBlack: '#444a73', brightRed: '#ff757f', brightGreen: '#c3e88d', brightYellow: '#ffc777', brightBlue: '#82aaff', brightMagenta: '#c099ff', brightCyan: '#86e1fc', brightWhite: '#c8d3f5',
  },
  'Solarized Dark': {
    bg: '#001e27', fg: '#708284', cursor: '#708284', chrome: '#00181f', selection: 'rgba(255,255,255,0.2)',
    black: '#002831', red: '#d11c24', green: '#738a05', yellow: '#a57706', blue: '#2176c7', magenta: '#c61c6f', cyan: '#259286', white: '#eae3cb',
    brightBlack: '#475b62', brightRed: '#bd3613', brightGreen: '#475b62', brightYellow: '#536870', brightBlue: '#708284', brightMagenta: '#5956ba', brightCyan: '#819090', brightWhite: '#fcf4dc',
  },
  'Snazzy': {
    bg: '#1e1f29', fg: '#ebece6', cursor: '#e4e4e4', chrome: '#181921', selection: 'rgba(255,255,255,0.2)',
    black: '#000000', red: '#fc4346', green: '#50fb7c', yellow: '#f0fb8c', blue: '#49baff', magenta: '#fc4cb4', cyan: '#8be9fe', white: '#ededec',
    brightBlack: '#555555', brightRed: '#fc4346', brightGreen: '#50fb7c', brightYellow: '#f0fb8c', brightBlue: '#49baff', brightMagenta: '#fc4cb4', brightCyan: '#8be9fe', brightWhite: '#ededec',
  },
  'Oceanic Next': {
    bg: '#162c35', fg: '#c0c5ce', cursor: '#c0c5ce', chrome: '#12232a', selection: 'rgba(255,255,255,0.2)',
    black: '#162c35', red: '#ec5f67', green: '#99c794', yellow: '#fac863', blue: '#6699cc', magenta: '#c594c5', cyan: '#5fb3b3', white: '#ffffff',
    brightBlack: '#65737e', brightRed: '#ec5f67', brightGreen: '#99c794', brightYellow: '#fac863', brightBlue: '#6699cc', brightMagenta: '#c594c5', brightCyan: '#5fb3b3', brightWhite: '#ffffff',
  },
  'Zenburn': {
    bg: '#3f3f3f', fg: '#dcdccc', cursor: '#73635a', chrome: '#323232', selection: 'rgba(255,255,255,0.2)',
    black: '#4d4d4d', red: '#7d5d5d', green: '#60b48a', yellow: '#f0dfaf', blue: '#5d6d7d', magenta: '#dc8cc3', cyan: '#8cd0d3', white: '#dcdccc',
    brightBlack: '#709080', brightRed: '#dca3a3', brightGreen: '#c3bf9f', brightYellow: '#e0cf9f', brightBlue: '#94bff3', brightMagenta: '#ec93d3', brightCyan: '#93e0e3', brightWhite: '#ffffff',
  },
  'Tomorrow Night': {
    bg: '#1d1f21', fg: '#c5c8c6', cursor: '#c5c8c6', chrome: '#17191a', selection: 'rgba(255,255,255,0.2)',
    black: '#000000', red: '#cc6666', green: '#b5bd68', yellow: '#f0c674', blue: '#81a2be', magenta: '#b294bb', cyan: '#8abeb7', white: '#ffffff',
    brightBlack: '#4c4c4c', brightRed: '#cc6666', brightGreen: '#b5bd68', brightYellow: '#f0c674', brightBlue: '#81a2be', brightMagenta: '#b294bb', brightCyan: '#8abeb7', brightWhite: '#ffffff',
  },
  'GitHub Light': {
    bg: '#ffffff', fg: '#1f2328', cursor: '#0969da', chrome: '#f5f5f5', selection: 'rgba(0,0,0,0.15)',
    black: '#24292f', red: '#cf222e', green: '#116329', yellow: '#4d2d00', blue: '#0969da', magenta: '#8250df', cyan: '#1b7c83', white: '#6e7781',
    brightBlack: '#57606a', brightRed: '#a40e26', brightGreen: '#1a7f37', brightYellow: '#633c01', brightBlue: '#218bff', brightMagenta: '#a475f9', brightCyan: '#3192aa', brightWhite: '#8c959f',
  },
  'Monokai Pro': {
    bg: '#2d2a2e', fg: '#fcfcfa', cursor: '#c1c0c0', chrome: '#242225', selection: 'rgba(255,255,255,0.2)',
    black: '#2d2a2e', red: '#ff6188', green: '#a9dc76', yellow: '#ffd866', blue: '#fc9867', magenta: '#ab9df2', cyan: '#78dce8', white: '#fcfcfa',
    brightBlack: '#727072', brightRed: '#ff6188', brightGreen: '#a9dc76', brightYellow: '#ffd866', brightBlue: '#fc9867', brightMagenta: '#ab9df2', brightCyan: '#78dce8', brightWhite: '#fcfcfa',
  },
  'Flexoki Dark': {
    bg: '#100f0f', fg: '#cecdc3', cursor: '#cecdc3', chrome: '#0d0c0c', selection: 'rgba(255,255,255,0.2)',
    black: '#100f0f', red: '#d14d41', green: '#879a39', yellow: '#d0a215', blue: '#4385be', magenta: '#ce5d97', cyan: '#3aa99f', white: '#878580',
    brightBlack: '#575653', brightRed: '#af3029', brightGreen: '#66800b', brightYellow: '#ad8301', brightBlue: '#205ea6', brightMagenta: '#a02f6f', brightCyan: '#24837b', brightWhite: '#cecdc3',
  },
  'Alabaster': {
    bg: '#f7f7f7', fg: '#000000', cursor: '#007acc', chrome: '#ededed', selection: 'rgba(0,0,0,0.15)',
    black: '#000000', red: '#aa3731', green: '#448c27', yellow: '#cb9000', blue: '#325cc0', magenta: '#7a3e9d', cyan: '#0083b2', white: '#b7b7b7',
    brightBlack: '#777777', brightRed: '#f05050', brightGreen: '#60cb00', brightYellow: '#f2af50', brightBlue: '#007acc', brightMagenta: '#e64ce6', brightCyan: '#00aacb', brightWhite: '#f7f7f7',
  },
  'Embers Dark': {
    bg: '#16130f', fg: '#a39a90', cursor: '#a39a90', chrome: '#120f0c', selection: 'rgba(255,255,255,0.2)',
    black: '#16130f', red: '#826d57', green: '#57826d', yellow: '#6d8257', blue: '#6d5782', magenta: '#82576d', cyan: '#576d82', white: '#a39a90',
    brightBlack: '#5a5047', brightRed: '#828257', brightGreen: '#464039', brightYellow: '#50483f', brightBlue: '#8a8075', brightMagenta: '#beb6ae', brightCyan: '#825757', brightWhite: '#dbd6d1',
  },
  'Vesper': {
    bg: '#101010', fg: '#ffffff', cursor: '#acb1ab', chrome: '#0d0d0d', selection: 'rgba(255,255,255,0.2)',
    black: '#101010', red: '#f5a191', green: '#90b99f', yellow: '#e6b99d', blue: '#aca1cf', magenta: '#e29eca', cyan: '#ea83a5', white: '#a0a0a0',
    brightBlack: '#7e7e7e', brightRed: '#ff8080', brightGreen: '#99ffe4', brightYellow: '#ffc799', brightBlue: '#b9aeda', brightMagenta: '#ecaad6', brightCyan: '#f591b2', brightWhite: '#ffffff',
  },
};
