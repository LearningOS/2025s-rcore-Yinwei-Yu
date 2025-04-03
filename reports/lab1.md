# ch3实验报告

## 学习轨迹

[ch3-day1](https://yinwei-yu.github.io/2025/04/01/day3/)
[ch3-day2](https://yinwei-yu.github.io/2025/04/02/day4/)

## 实验

完成作业要求功能,实现如下:

### TaskManager结构更改

为了记录每个任务的各个系统调用的调用次数,在TaskManagerInner中添加一个二维数组如下:

```rust
pub struct TaskManagerInner {
    //original code...
    /// times of syscall with id
    current_syscall_times: Vec<Vec<usize>>,//修改在这里
}
```

第一个索引为`current_task`即当前运行的任务id,第二个索引是进程号.这里本来想使用HashMap但是没有标准库好像实现不了?vec导致空间占有太大了,可能后期需要修改,现在先按下不表吧.

在初始化时加入如下操作:

```rust
let max_syscall_id = 500;
        let mut current_syscall_times =Vec::with_capacity(max_syscall_id);
        for _ in 0..num_app {
            current_syscall_times.push(vec![0;max_syscall_id]);
        }
```
这个操作很简单,直接初始化为num_app个长度为max_syscall的元素都为0的数组,缺点是max_syscall写死了,而且占用空间大.

### current_syscall_times的更改

为了实现系统调用次数的更新和获得这个数据,添加两个函数,因为要给syscall模块使用,所以要声明为pub

```rust
impl TaskManager {
  ///add 1 to syscall with id's count
    pub fn increment_syscall_count(&self, syscall_id: usize) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.current_syscall_times[current][syscall_id] += 1;
    }

    ///get task_syscall_count
    pub fn get_task_syscall_count(&self, syscall_id: usize) -> usize {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.current_syscall_times[current][syscall_id]
    }
}
```

这两个函数的实现也比较简单,获取内部可变的控制器本身,然后修改相应数据.

## syscall的更改

为了记录具体系统调用的调用次数,只需要在syscall中先对指定sys_call_id加1即可:

```rust
pub fn syscall(syscall_id: usize, args: [usize; 3]) -> isize {
    TASK_MANAGER.increment_syscall_count(syscall_id);
    //original code...
}
```

## trace系统调用的实现

按照文档要求实现即可:

```rust
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    match trace_request {
        0 => unsafe{
            let value = *(id as *const u8);
            return value as isize;
        }
        1 => unsafe {
            let addr = id as *mut u8;
            *addr = data as u8;
            return 0 as isize;
        },
        2 => {
            return TASK_MANAGER.get_task_syscall_count(id) as isize;
        }
        _ => -1,
    }
}
```

做的时候遇到的坑是:`trace_request`为0时,刚开始直接返回了`id as *const u8`,这是直接返回了地址,没有返回地址指向位置的值QAQ

其他功能倒是不难

## 简答题

### 1

```rust
///ch2b_bad_address.rs
pub fn main() -> isize {
    unsafe {
        #[allow(clippy::zero_ptr)]
        (0x0 as *mut u8).write_volatile(0);
    }
    panic!("FAIL: T.T\n");
}
```

错误信息:

> [kernel] PageFault in application, bad addr = 0x0, bad instruction = 0x804003a4, kernel killed it.

因为在用户态非法访问了0x0地址

```rust
///ch2b_bad_instructions.rs
#[no_mangle]
pub fn main() -> ! {
    unsafe {
        core::arch::asm!("sret");
    }
    panic!("FAIL: T.T\n");
}
```

错误信息:

>[kernel] IllegalInstruction in application, kernel killed it.

错误原因:sret为从S特权级返回U特权级的指令,用户态无法调用

```rust
///ch2_bad_register.rs
pub fn main() -> ! {
    let mut sstatus: usize;
    unsafe {
        core::arch::asm!("csrr {}, sstatus", out(reg) sstatus);
    }
    panic!("(-_-) I get sstatus:{:x}\nFAIL: T.T\n", sstatus);
}
```

报错内容和上一个一样

原因是在用户态读取了csr寄存器，这是内核态才能读取的内容

### 2

1. 
刚进入__restore时,sp指向内核栈.
两个使用场景:在Trap结束后,通过__restore恢复应用上下文;在初始化应用上下文时,复用__restore来存储相关寄存器

2. 
