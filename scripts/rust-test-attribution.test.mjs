import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { unattributedTests } from './rust-test-attribution.mjs';

test('only inline test modules are inspected; long attached docs retain attribution', () => {
  const source = `#[cfg(test)] mod external;
    fn production_banner() {}
    #[cfg(test)] mod tests {
      #[test]
      /// ${'A long explanation of the regression. '.repeat(30)}
      fn documented() {}
      fn orphan() {}
    }
    fn production_tail() {}`;
  assert.deepEqual(unattributedTests(source), ['orphan']);
});

test('literals, nested comments and earlier test attributes cannot hide disabled tests', () => {
  const source = `const EXAMPLE: &str = r###"#[cfg(test)] mod fake { fn phantom() {} }"###;
    #[cfg(test)] mod tests {
      #[test] fn real() { let text = r#"}"#; let brace = '}'; }
      /* } /* { */ fn commented() {} */
      fn disabled() {}
      #[tokio::test] async fn async_real() {}
      mod nested { fn nested_orphan() {} }
      fn helper_with_args(value: &str) {}
      fn helper_return() -> bool { true }
    }`;
  assert.deepEqual(unattributedTests(source), ['disabled', 'nested_orphan']);
});

test('real foundation/main regressions pass while removing a test attribute is detected', () => {
  const read = (file) => readFileSync(new URL(`../src-tauri/src/${file}.rs`, import.meta.url), 'utf8');
  assert.deepEqual(unattributedTests(read('foundation')), []);
  assert.deepEqual(unattributedTests(read('main')), []);
  const veto = read('veto');
  assert.deepEqual(unattributedTests(veto), []);
  const disabled = veto.replace(/#\[test\]\s*(?=fn a_call_that_cannot_be_recorded_is_refused)/, '');
  assert.notEqual(disabled, veto);
  assert.deepEqual(unattributedTests(disabled), ['a_call_that_cannot_be_recorded_is_refused']);
});
