use std::sync::Arc;

use chaos_tests::SpinMutex;

// COPIED from chaos-tests/tests/basic/group_01.rs
fn run_with_timeout<F: FnOnce() + Send + 'static>(f: F, ms: u64) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || { f(); let _ = tx.send(()); });
    rx.recv_timeout(std::time::Duration::from_millis(ms)).is_ok()
}

// HUMAN
#[test]
fn spin_mutex_basic() {
  let mutex = SpinMutex::new(0);
  {
    let mut guard = mutex.lock();
    *guard += 1;
  }
  {
    let guard = mutex.lock();
    assert_eq!(*guard, 1);
  }
}

// HUMAN
#[test]
fn timeout_on_lock() {
  let mutex = SpinMutex::new(0);
  let done = run_with_timeout(move || {
    let _guard = mutex.lock();
    mutex.lock();
  }, 500);
  assert!(!done);
}

// AGENT
#[test]
fn parallel() {
  let mutex = Arc::new(SpinMutex::new(0));
  let handles: Vec<_> = (0..1024).map(|_| {
    let m = mutex.clone();
    std::thread::spawn(move || {
      for _ in 0..100 {
        let mut guard = m.lock();
        *guard += 1;
      }
    })
  }).collect();
  for h in handles {
    h.join().unwrap();
  }
  assert_eq!(*mutex.lock(), 1024 * 100);
}
