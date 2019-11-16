use crate::ArrayVec;

use core::mem;
use core::ptr;
use core::ops::Range;
use core::iter::{DoubleEndedIterator, FusedIterator};

#[derive(Debug, PartialEq)]
pub struct Drain<'a, T, const N: usize> {
  inner: &'a mut ArrayVec<T, { N }>,
  /// The index of the first item being removed. 
  drain_range_start: usize,
  /// The index of the first item after the drained range. 
  tail_start: usize,
  tail_length: usize, 
  /// The front of the remaining drained range. 
  head: *mut T,
  /// One after the last item in the range being drained. 
  tail: *mut T, 
}

impl<'a, T, const N: usize> Drain<'a, T, { N }> {
  pub(crate) fn with_range(vector: &'a mut ArrayVec<T, { N }>, range: Range<usize>) -> Self {
    debug_assert!(range.start <= range.end, "The range start must be before end");
    debug_assert!(range.end <= vector.len(), "The range is out of bounds");
    debug_assert!(core::mem::size_of::<T>() != 0, "We can't deal with zero-size types");

    unsafe {
      let head = vector.as_mut_ptr().add(range.start);
      let tail = vector.as_mut_ptr().add(range.end);
      let tail_length = vector.len() - (range.end - range.start);

      // prevent a leaked Drain from letting users read from uninitialized memory 
      vector.set_len(range.start);

      Drain {
        inner: vector,
        drain_range_start: range.start,
        tail_start: range.end,
        tail_length,
        head,
        tail,
      }
    }
  }
}

impl<'a, T, const N: usize> Iterator for Drain<'a, T, { N }> {
  type Item = T;

  fn next(&mut self) -> Option<Self::Item> {
    if self.head == self.tail {
      return None;
    }

    unsafe {
      let item = self.head.read();
      self.head = self.head.add(1);
      Some(item)
    }
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    (self.len(), Some(self.len()))
  }
}

impl<'a, T, const N: usize> DoubleEndedIterator for Drain<'a, T, { N }> {
  fn next_back(&mut self) -> Option<Self::Item> {
    if self.head == self.tail {
      return None;
    }

    unsafe {
      self.tail = self.tail.sub(1);
      let item = self.tail.read();
      Some(item)
    }
  }
}

impl<'a, T, const N: usize> ExactSizeIterator for Drain<'a, T, { N }> {
  fn len(&self) -> usize {
    let size = mem::size_of::<T>();
    assert!(0 < size && size <= isize::max_value() as usize);

    let difference = (self.tail as isize) - (self.head as isize);
    debug_assert!(difference >= 0, "Tail should always be after head");

    difference as usize / size
  }
}

impl<'a, T, const N: usize> FusedIterator for Drain<'a, T, { N }> {}

impl<'a, T, const N: usize> Drop for Drain<'a, T, { N }> {
  fn drop(&mut self) {
    // remove any remaining items so their destructors can run 
    while let Some(item) = self.next() {
      mem::drop(item);
    }

    if self.tail_length == 0 {
      return;
    }

    unsafe {
      let tail_start = self.inner.as_ptr().add(self.tail_start);
      let drain_range_start = self.inner.as_mut_ptr().add(self.drain_range_start);

      // moves the tail (the items after the drained range) forwards now that 
      // the drained items are destroyed 
      ptr::copy(tail_start, drain_range_start, self.tail_length);

      // we can now update the length 
      self.inner.set_len(self.drain_range_start + self.tail_length);
    }
  }
}