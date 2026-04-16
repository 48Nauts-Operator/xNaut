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
};
