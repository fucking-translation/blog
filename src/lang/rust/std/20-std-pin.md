# std::pin

[原文](https://doc.rust-lang.org/std/pin/index.html)

定义：一种将数据固定在内存中的类型

## 描述

从对象在内存中位置不变的意义上来说，保证对象不移动有时很有用。这种情况的一个主要示例就是构建`自引用`结构，因为使用指向自身的指针移动对象会使他们无效，这可能导致未定义的行为。

从较高的层次来说，`Pin<P>`可以确保任何类型的指针`P`在内存中都有固定的位置，这意味着它不能被移动到其他地方，并且在删除之前不能释放其内存。当讨论将固定数据和非固定数据结合在一起组成类型时，事情将变得更加微妙。

默认情况下，Rust中所有类型都是可移动的。Rust允许按值传递类型。普通的智能指针类型(例如Box<T>或者&mut T)允许替换和移动他们包含的值：可以将其移出`Box<T>`或者使用`mem::swap`。`Pin<P>`封装了一个指针类型P，因此`Pin<Box<T>>`功能和常规类型`Box<T>`非常相似。当`Pin<Box<T>>`被销毁时，其包含的内容也将被销毁，并且其占用的内存也被释放。同样的，`Pin<&mut T>`和`&mut T`非常像。但是`Pin<P>`不会让使用者真的去包含`Box<T>`或者`&mut T`来固定数据，这意味着不能使用`mem::swap`之类的操作。

```rust
use std::pin::Pin;
fn swap_pins<T>(x: Pin<&mut T>, y: Pin<&mut T>) {
    // `mem::swap` needs `&mut T`, but we cannot get it.
    // We are stuck, we cannot swap the contents of these references.
    // We could use `Pin::get_unchecked_mut`, but that is unsafe for a reason:
    // we are not allowed to use it for moving things out of the `Pin`.
}
```

值得重申的是，`Pin<P>`不会改变Rust编译器允许所有类型都是可移动的事实。`mem::swap`依然可以调用任意类型`T`。取而代之的是，`Pin<P>`使调用需要`&mut T`的方法变得不可能来阻止一些（由`Pin<P>`封装的指针所指向的）值被移动（如`mem::swap`）。

`Pin<P>`可用于包装任何类型的指针P，因此他可以与`Deref`和`DerefMut`交互。`Pin<P> where P: Deref`应该被视为固定`P::Target`的P型指针。因此，`Pin<Box<T>>`是一个固定T的指针，`Pin<Rc<T>>`是固定T的指针计数器。为了确保正确性，`Pin<P>`依赖`Deref`和`DerefMut`的实现使其不能移出其自身参数，并且总是只返回一个指向固定数据的Pin指针。


## Unpin
即使不固定，许多类型也始终可以自由移动，因为他们不依赖于具有稳定的地址。这包括所有基本类型（如`bool`，`i32`和引用类型）以及仅用这些类型组成的类型。不需要固定的类型将实现`Unpin`特征，从而取消了`Pin<P>`的影响。对于`T: Unpin`，`Pin<Box<T>>`和`Box<T>`的功能相同，`Pin<&mut T>`和`&mut T`的作用相同。

值得注意的是，`Pin`和`Unpin`只影响指向类型`P::Target`，而不影响封装在`Pin<P>`中的P类型本身。例如，`Box<T>`是否为`Unpin`对`Pin<Box<T>>`的行为没有影响

## 例子1: 自引用结构(self-referential struct)

在进一步解释`Pin<T>`之前，先讨论如何使用它的一些示例。

```rust
use std::pin::Pin;
use std::marker::PhantomPinned;
use std::ptr::NonNull;

// This is a self-referential struct because the slice field points to the data field.
// We cannot inform the compiler about that with a normal reference,
// as this pattern cannot be described with the usual borrowing rules.
// Instead we use a raw pointer, though one which is known not to be null,
// as we know it's pointing at the string.
struct Unmovable {
    data: String,
    slice: NonNull<String>,
    _pin: PhantomPinned,
}

impl Unmovable {
    // To ensure the data doesn't move when the function returns,
    // we place it in the heap where it will stay for the lifetime of the object,
    // and the only way to access it would be through a pointer to it.
    fn new(data: String) -> Pin<Box<Self>> {
        let res = Unmovable {
            data,
            // we only create the pointer once the data is in place
            // otherwise it will have already moved before we even started
            slice: NonNull::dangling(),
            _pin: PhantomPinned,
        };
        let mut boxed = Box::pin(res);

        let slice = NonNull::from(&boxed.data);
        // we know this is safe because modifying a field doesn't move the whole struct
        unsafe {
            let mut_ref: Pin<&mut Self> = Pin::as_mut(&mut boxed);
            Pin::get_unchecked_mut(mut_ref).slice = slice;
        }
        boxed
    }
}

let unmoved = Unmovable::new("hello".to_string());
// The pointer should point to the correct location,
// so long as the struct hasn't moved.
// Meanwhile, we are free to move the pointer around.
let mut still_unmoved = unmoved;
assert_eq!(still_unmoved.slice, NonNull::from(&still_unmoved.data));

// Since our type doesn't implement Unpin, this will fail to compile:
// let mut new_unmoved = Unmovable::new("world".to_string());
// std::mem::swap(&mut *still_unmoved, &mut *new_unmoved);
```

## 例子2: 侵入式双向链表(intrusive doubly-linked list)

在侵入式双向链表中，不会真的为表中的元素分配内存空间，如何分配由使用者控制，并且元素可以分配在（比链表存活时间更短暂的）栈帧上。
双向链表中每个元素都有指针分别指向它的前驱和后继元素。只有当链表中的元素位置在内存中都是固定的才可以添加元素，因为移动一个元素可能会使指针失效。并且，链表中的元素实现`Drop`会在它被移除出链表时，修复指向它前驱和后继元素的指针。
至关重要的是，如果一个元素在不调用`Drop::drop`的情况下被释放或者以其他方式失效，则其相邻元素指向该元素的指针将变为无效，这将破坏数据结构。
因此，`Pin`也许附带`Drop`相关的保证。

## Drop保证

`Pin`的目的是为了能够依赖内存中数据的位置。因此不仅限制了移动数据，还限制了用于存储数据的内存的重新分配。具体来说，对于固定的数据，从固定到`drop`被调用的那一刻之前，它的内存都不会失效或者被重新利用，只有当`drop`返回或者`panic`，该内存才可以重用。
内存可以因为被释放而“失效”，也可以用`None`来代替`Some(v)`，或者调用`Vec::set_len`来`消灭`数组中的元素。可以通过`ptr::write`覆写来重新利用它，而无需事先调用析构函数。未经调用`drop`的固定数据均不允许使用这些方法。
注意这个保证并不意味着不会产生内存泄漏！你依然可以不去调用固定元素的`drop`方法（如：你可以对`Pin<Box<T>>`使用`mem::forget`方法），在双向列表的示例中，元素将只保存在列表中，但是如果不调用`drop`方法，将无法释放或重新使用内存。

## Drop实现

如果你的数据类型使用`Pin`，在实现`Drop`时需要格外注意，`drop`函数携带`&mut self`参数，即使你的类型先前已经固定，也将调用该函数，好像编译器会自动调用`Pin::get_unchecked_mut`。
这永远不会在安全代码中引起问题，因为实现需要固定的类型需要`unsafe`的代码。请注意，如果在你的类型中使用`Pin`（如在`Pin<&Self>`或`Pin<&mut Self>`）会对`Drop`实现产生影响:如果你的类型中的一个元素已经被固定，你必须将`Drop`视为隐式获取`Pin<&mut Self>`。

举个例子，你可以像如下代码一样实现`Drop`：

```rust
impl Drop for Type {
    fn drop(&mut self) {
        // `new_unchecked` is okay because we know this value is never used
        // again after being dropped.
        inner_drop(unsafe { Pin::new_unchecked(self)});
        fn inner_drop(this: Pin<&mut Type>) {
            // Actual drop code goes here.
        }
    }
}
```

`inner_drop`函数具有`drop`应有的类型，所以这确保你不会偶发的在这种与`Pin`互相矛盾的方式中使用`self/this`。

而且，如果你的字段被`#[repr(packed)]`修饰，编译器将自动移除字段。它甚至可以对恰好对齐的字段执行此操作。所以你不能将`Pin`和`#[repr(packed)]`一起使用。

## 投影和结构固定 

在处理固定结构时，会遇到一个问题，如何访问方法中结构（`Pin<&mut Struct>`）的字段。通常是写辅助方法(即投影)将`Pin<&mut Struct>`转化成字段中的一个引用。但是该引用应该具有什么类型？是`Pin<&mut Field>`还是`&mut Field`呢？枚举字段和容器字段（`Vec<T>`）或者封装类型（`Box<T>`或`RefCell<T>`）也会出现相同的问题（这个问题适用于可变引用和共享引用，我们仅在此处使用可变引用的更常见情况进行说明）。

实际上，是由数据结构的作者决定是否为特殊的字段固定投影来将`Pin<&mut Struct>`转换为`Pin<&mut Field>`或者`&mut Field`。这里有一些限制，最大的限制就是`一致性`：每个字段都可以被投射到固定的引用上，或者作为投影的一部分去掉了`Pin`。如果同一个字段同时满足这两个条件将是有问题的。

作为数据结构的作者，你需要为每个字段决定是否需要将`Pin`(的影响)“传播”到这个字段上。传播中的`Pin`也被称为结构化，因为它遵循该类型的结构。在以下小结中，我们描述了两种选择都必须考虑的因素。