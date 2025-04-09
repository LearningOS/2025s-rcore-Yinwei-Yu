# ch3实验报告

## 学习轨迹

[ch3-day1](https://yinwei-yu.github.io/2025/04/01/day3/)
[ch3-day2](https://yinwei-yu.github.io/2025/04/02/day4/)

## 实验

完成作业要求功能,实现如下:

### TaskManager结构更改

为了记录每个任务的各个系统调用的调用次数,在TaskManagerInner中添加一个二维数组

第一个索引为`current_task`即当前运行的任务id,第二个索引是进程号.这里本来想使用HashMap但是没有标准库好像实现不了?vec导致空间占有太大了,可能后期需要修改,现在先按下不表吧.

在初始化时直接初始化为num_app个长度为max_syscall的元素都为0的数组,缺点是max_syscall写死了,而且占用空间大.

为了实现系统调用次数的更新和获得这个数据,添加两个函数完成递增和读取,因为要给syscall模块使用,所以要声明为pub

为了记录具体系统调用的调用次数,只需要在syscall中先对指定sys_call_id加1即可:

做的时候遇到的坑是:`trace_request`为0时,刚开始直接返回了`id as *const u8`,这是直接返回了地址,没有返回地址指向位置的值QAQ


## 简答题

### 1

ch2b_bad_address.rs

错误信息:

> [kernel] PageFault in application, bad addr = 0x0, bad instruction = 0x804003a4, kernel killed it.

因为在用户态非法访问了0x0地址

---  

ch2b_bad_instructions.rs

错误信息:

>[kernel] IllegalInstruction in application, kernel killed it.

错误原因:sret为从S特权级返回U特权级的指令,用户态无法调用

---  

ch2_bad_register.rs

报错内容和上一个一样

原因是在用户态读取了csr寄存器，这是内核态才能读取的内容

> sbi版本:RustSBI version 0.3.0-alpha.2, adapting to RISC-V SBI v1.0.0

### 2

1. 
刚进入__restore时,sp指向内核栈.
两个使用场景:在Trap结束后,通过__restore恢复应用上下文;在初始化应用上下文时,复用__restore来存储相关寄存器

2. 
恢复先前保存的sstatus,sepc,sscratch寄存器
sstatus寄存器的特定位保存了当前所处的特权级
sepc寄存器保存了trap指令的下一条代码用于返回后继续执行原程序
sscratch寄存器是一个中间寄存器用于实现内核栈和用户栈的转换

3. 
x2寄存器又称sp,是栈指针,在后面保存了
x4寄存器是线程指针,用户程序不会使用,所以没有保存

4. 
sp指向用户栈,sscratch指向内核栈

5. 
`csrw sstatus, t0`
因为在第29,31行把该寄存器中保存的用户态信息存到栈中,现在从栈中恢复了用户态的信息

6. 
sp指向内核栈,sscratch指向用户栈

7. 
ecall指令

## 荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与以下各位就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

> 暂无

2. 此外，我也参考了以下资料,还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

> claude 3.7提供的debug信息

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。