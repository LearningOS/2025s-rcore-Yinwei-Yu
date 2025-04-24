# ch5实验报告

## 实现功能

为processor添加获取页表的函数用于mmap和unmap

为实现spawn,将主要逻辑放在TaskControl中,避免了许多所有权和不可见性问题.spawn函数结合了fork和exec,先从文件名中解析elf信息,如果成功,则新建一块空的地址空间,而不是复制父进程的地址空间,然后初始化trap上下文后把TCB添加到就绪队列中即可

为实现优先级调度,先在TCBinner中添加stride,pass两个字段.然后在fetch函数中实现相关功能.采用了简单的暴力搜索.

## 问答题

因为每次调度都会选择最小的stride进行调度和更新,则MAX_Stride和MIN_Stride之间最多相差一个pass,又pass<=BIGSTRIDE/priority,priority>=2,那么pass<=BIGSTRIDE/2,从而有结论成立

```rust
use core::cmp::Ordering;

struct Stride(u64);

impl PartialOrd for Stride {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.0 as i64).cmp(&(other.0 as i64)) //利用有符号整数特性,大于MAX/2的数会被等价为负数
    }
}

impl PartialEq for Stride {
    fn eq(&self, other: &Self) -> bool {
        false
    }
}
```

## 荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与以下各位就（与本次实验相关的）以下
方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

无

2. 此外，我也参考了以下资料，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

无

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。我清楚地知道，从以上方面获
得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。我未曾也不会向他人
（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。我提交至本实验的评测系统
的代码，均无意于破坏或妨碍任何计算机系统的正常运转。我清楚地知道，以上情况均为本课程纪律所
禁止，若违反，对应的实验成绩将按”-100”分计。