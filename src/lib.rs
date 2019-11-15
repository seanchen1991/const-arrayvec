//! An implementation of the [arrayvec](https://crates.io/crates/arrayvec) crate
//! using *Const Generics*.

#![no_std]
#![feature(const_generics)]

use core::ptr;
use core::slice;
use core::cmp::Ordering;
use core::hash::{Hash, Hasher};
use core::mem::{self, MaybeUninit};
use core::fmt::{self, Debug, Display, Formatter};
use core::ops::{Deref, DerefMut, Index, IndexMut};

macro_rules! out_of_bounds {
  ($method:expr, $index:expr, $len:expr) => {
    panic!(
      concat!(
        "ArrayVec::",
        $method,
        "(): index {} is out of bounds in vector of length {}"
      ),
      $index, $len
    );
  };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CapacityError<T>(pub T);

impl<T> Display for CapacityError<T> {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    write!(f, "Insufficient capacity")
  }
}

pub struct ArrayVec<T, const N: usize> {
  items: [MaybeUninit<T>; N],
  length: usize,
}

impl<T, const N: usize> ArrayVec<T, { N }> {
  pub fn new() -> Self {
    unsafe {
      ArrayVec {
        // this is safe because we've asked for a big block of
        // uninitialized memory which will be treated as an 
        // array of uninitialized items, which is perfectly 
        // valid for [MaybeUninit<_>; N]
        items: MaybeUninit::uninit().assume_init(),
        length: 0,
      }
    }
  }

  pub const fn len(&self) -> usize { self.length }  

  pub const fn is_empty(&self) -> bool { self.len() == 0 }

  pub const fn capacity(&self) -> usize { N }

  pub const fn is_full(&self) -> bool { self.len() == self.capacity() }

  pub const fn remaining_capacity(&self) -> usize { self.capacity() - self.len() }

  pub fn as_ptr(&self) -> *const T { self.items.as_ptr() as *const T }

  pub fn as_mut_ptr(&mut self) -> *mut T { self.items.as_mut_ptr() as *mut T }

  pub fn as_slice(&self) -> &[T] { self.deref() }

  pub fn as_slice_mut(&mut self) -> &mut [T] { self.deref_mut() }

  /// Add an item to the end of the array without checking capacity. 
  /// 
  /// # Safety
  /// 
  /// It is up to the caller to ensure the vector's capacity is suitably large.
  /// 
  /// This method uses *debug assertions* to detect overflows in debug builds. 
  pub unsafe fn push_unchecked(&mut self, item: T) {
    debug_assert!(!self.is_full());
    let len = self.len();

    // index into the underlying array using pointer arithmetic and write 
    // the item to the correct spot 
    self.as_mut_ptr().add(len).write(item);

    self.set_len(len + 1);
  }

  /// Set the vector's length without dropping or moving out elements. 
  /// 
  /// # Safety
  /// 
  /// This method is unsafe because it changes the number of valid elements 
  /// the vector thinks it contains, without adding or removing any elements. 
  pub unsafe fn set_len(&mut self, new_length: usize) {
    debug_assert!(new_length <= self.capacity());
    self.length = new_length;
  }

  /// Add an item to the end of the vector. 
  /// 
  /// # Panics
  /// 
  /// The vector must have enough room for the new item. 
  /// 
  /// # Examples 
  /// 
  /// ```rust
  /// use const_arrayvec::ArrayVec;
  /// let mut vector: ArrayVec<u32, 5> = ArrayVec::new();
  /// 
  /// assert!(vector.is_empty());
  /// 
  /// vector.push(42);
  /// 
  /// assert_eq!(vector.len(), 1);
  /// assert_eq!(vector[0], 42);
  /// ```
  pub fn push(&mut self, item: T) {
    match self.try_push(item) {
      Ok(_)  => {},
      Err(e) => panic!("Push failed: {}", e),
    }
  }

  /// Try to add an item to the end of the vector, returning the 
  /// original item if there wasn't enough room. 
  /// 
  /// # Examples 
  /// 
  /// ```rust 
  /// use const_arrayvec::{ArrayVec, CapacityError};
  /// let mut vector: ArrayVec<u32, 2> = ArrayVec::new();
  /// 
  /// assert!(vector.try_push(1).is_ok());
  /// assert!(vector.try_push(2).is_ok());
  /// assert!(vector.is_full());
  /// 
  /// assert_eq!(vector.try_push(42), Err(CapacityError(42)));
  /// ```
  pub fn try_push(&mut self, item: T) -> Result<(), CapacityError<T>> {
    if self.is_full() {
      Err(CapacityError(item))
    } else {
      unsafe {
        self.push_unchecked(item);
        Ok(())
      }
    }
  }

  /// Remove an item from the end of the vector. 
  /// 
  /// # Examples 
  /// 
  /// ```rust
  /// use const_arrayvec::ArrayVec;
  /// let mut vector: ArrayVec<u32, 5> = ArrayVec::new();
  /// 
  /// vector.push(12);
  /// vector.push(34);
  /// 
  /// assert_eq!(vector.len(), 2);
  /// 
  /// let got = vector.pop();
  /// 
  /// assert_eq!(got, Some(34));
  /// assert_eq!(vector.len(), 1);
  /// ```
  pub fn pop(&mut self) -> Option<T> {
    if self.is_empty() {
      return None;
    }

    unsafe {
      let new_length = self.len() - 1;
      self.set_len(new_length);
      Some(ptr::read(self.as_ptr().add(new_length)))
    }
  }

  /// Shorten the vector, keeping the first `new_length` elements 
  /// and dropping the rest.
  pub fn truncate(&mut self, new_length: usize) {
    unsafe {
      if new_length < self.len() {
        let start = self.as_mut_ptr().add(new_length);
        let num_elements_to_remove = self.len() - new_length;
        let tail: *mut [T] = slice::from_raw_parts_mut(start, num_elements_to_remove);
        self.set_len(new_length);
        ptr::drop_in_place(tail);
      }
    }
  }

  /// Remove all items from the vector. 
  pub fn clear(&mut self) { self.truncate(0); }

  pub fn try_insert(&mut self, index: usize, item: T) -> Result<(), CapacityError<T>> {
    let len = self.len();

    // bounds checks 
    if index > self.len() {
      out_of_bounds!("try_insert", index, len);
    }

    if self.is_full() {
      return Err(CapacityError(item));
    }

    unsafe {
      // the spot to put the new value 
      let p = self.as_mut_ptr().add(index);
      // shift everything over to make space 
      // duplicating the `index`th element into two consecutive place 
      ptr::copy(p, p.offset(1), len - index);
      // write it in, overwriting the first copy of the `index`th element 
      ptr::write(p, item);
      // update the length 
      self.set_len(len + 1);
    }

    Ok(())
  }

  pub fn insert(&mut self, index: usize, item: T) {
    match self.try_insert(index, item) {
      Ok(_)  => {},
      Err(e) => panic!("Insert failed: {}", e),
    }
  }

  pub fn try_extend_from_slice(&mut self, other: &[T]) -> Result<(), CapacityError<()>>
    where T: Copy
  {
    if self.remaining_capacity() < other.len() {
      return Err(CapacityError(()));
    }

    let self_len = self.len();
    let other_len = other.len();

    unsafe {
      let dst = self.as_mut_ptr().offset(self_len as isize);
      // note: we have a mutable reference to self, so it's not possible 
      // for the two arrays to overlap 
      ptr::copy_nonoverlapping(other.as_ptr(), dst, other_len);
      self.set_len(self_len + other_len);
    }

    Ok(())
  }
}

impl<T, const N: usize> Drop for ArrayVec<T, { N }> {
  fn drop(&mut self) {
    // makes sure the destructors for all items are run 
    self.clear();
  }
}

impl<T, const N: usize> Deref for ArrayVec<T, { N }> {
  type Target = [T];

  fn deref(&self) -> &Self::Target {
    unsafe { slice::from_raw_parts(self.as_ptr(), self.len()) }
  }
}

impl<T, const N: usize> DerefMut for ArrayVec<T, { N }> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    unsafe { slice::from_raw_parts_mut(self.as_mut_ptr(), self.len()) }
  }
}

impl<T, const N: usize> AsRef<[T]> for ArrayVec<T, { N }> {
  fn as_ref(&self) -> &[T] { self.as_slice() }
}

impl<T, const N: usize> AsMut<[T]> for ArrayVec<T, { N }> {
  fn as_mut(&mut self) -> &mut [T] { self.as_slice_mut() }
}

impl<T: Debug, const N: usize> Debug for ArrayVec<T, { N }> {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    f.debug_list().entries(self.as_slice()).finish()
  }
}

impl <T: PartialEq, const N: usize> PartialEq<[T]> for ArrayVec<T, { N }> {
  fn eq(&self, other: &[T]) -> bool { self.as_slice() == other }
}

impl<T: PartialEq, const N: usize, const M: usize> PartialEq<ArrayVec<T, { M }>> for ArrayVec<T, { N }> {
  fn eq(&self, other: &ArrayVec<T, { M }>) -> bool {
    self.as_slice() == other.as_slice()
  }
}

impl<T: Eq, const N: usize> Eq for ArrayVec<T, { N }> {}

impl<T: PartialOrd, const N: usize> PartialOrd for ArrayVec<T, { N }> {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    self.as_slice().partial_cmp(other.as_slice())
  }
}

impl<T: Ord, const N: usize> Ord for ArrayVec<T, { N }> {
  fn cmp(&self, other: &Self) -> Ordering {
    self.as_slice().cmp(other.as_slice())
  }
}

impl<T: Hash, const N: usize> Hash for ArrayVec<T, { N }> {
  fn hash<H: Hasher>(&self, hasher: &mut H) { self.as_slice().hash(hasher); }
}

impl<T, const N: usize> Default for ArrayVec<T, { N }> {
  fn default() -> Self { ArrayVec::new() }
}

impl<T: Clone, const N: usize> Clone for ArrayVec<T, { N }> {
  fn clone(&self) -> ArrayVec<T, { N }> {
    let mut other: ArrayVec<T, { N }> = ArrayVec::new();

    for item in self.as_slice() {
      unsafe {
        // if it fits into the original, it'll fit into the clone 
        other.push_unchecked(item.clone());
      }
    }

    other
  }
}

impl<I, T, const N: usize> Index<I> for ArrayVec<T, { N }> 
  where [T]: Index<I>
{
  type Output = <[T] as Index<I>>::Output;

  fn index(&self, i: I) -> &Self::Output { self.as_slice().index(i) }
}

impl<I, T, const N: usize> IndexMut<I> for ArrayVec<T, { N }>
  where [T]: IndexMut<I>
{
  fn index_mut(&mut self, i: I) -> &mut Self::Output {
    self.as_slice_mut().index_mut(i)
  }
}

impl<T, const N: usize> From<[T; N]> for ArrayVec<T, { N }> {
  fn from(other: [T; N]) -> ArrayVec<T, { N }> {
    let mut vec = ArrayVec::<T, { N }>::new();

    unsafe {
      // copy the items from the array directly to the backing buffer 
      // note: safe because a [T; N] is identical to [MaybeUninit<T>; N]
      ptr::copy_nonoverlapping(other.as_ptr(), vec.as_mut_ptr(), other.len());

      // ownership has been transferred to the backing buffer, make sure the 
      // original array's destructors aren't called prematurely
      mem::forget(other);
      
      // the memory has now been initialized so it's safe to set the length 
      vec.set_len(N);
    }

    vec
  }
}