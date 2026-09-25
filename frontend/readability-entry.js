// Mozilla Readability, bundled offline for the Wiki tab (XNAUT-438).
//
// Borrowed work: @mozilla/readability 0.6.0, Apache 2.0, Copyright (c) 2010
// Arc90 Inc, https://github.com/mozilla/readability. Not invented here. It is
// the extractor Firefox Reader View uses, and rewriting it would be a worse
// version of thirty thousand lines of heuristics that already know what a docs
// page looks like.
//
// Bundled rather than loaded from a CDN because the Wiki tab has to work when
// the network is down, which is the whole point of the stored collection, and
// because a docs reader that fetches a script off the internet to read a page
// has the dependency exactly backwards. Same reasoning as the Monaco/loops
// bundle beside it.
//
// Built by `npm run build:wiki` into ../src/js/vendor/readability.bundle.js,
// which IS committed: a release build embeds the frontend, so a file that only
// exists after an npm install ships as a missing script.
//
// Departure from upstream: none. The entry exists only to name the global.
import { Readability, isProbablyReaderable } from '@mozilla/readability';

export { Readability, isProbablyReaderable };
