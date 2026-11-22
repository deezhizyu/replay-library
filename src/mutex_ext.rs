use std::sync::{Mutex, MutexGuard};

pub trait MutexExt<T> {
    fn lock_ignoring_poison(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn lock_ignoring_poison(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn lock_survives_a_panic_in_another_thread() {
        let mutex = Arc::new(Mutex::new(1));
        let poisoner = mutex.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock_ignoring_poison();
            panic!("poison the mutex");
        })
        .join();
        assert_eq!(*mutex.lock_ignoring_poison(), 1);
    }
}
