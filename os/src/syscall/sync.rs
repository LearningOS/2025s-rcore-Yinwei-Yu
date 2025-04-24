use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
// 定义死锁错误码
const DEADLOCK_ERROR: isize = -0xDEAD;

// 检测互斥锁死锁
fn detect_mutex_deadlock(mutex_id: usize) -> bool {
    let task = current_task().unwrap();
    let process = task.process.upgrade().unwrap();
    let process_inner = process.inner_exclusive_access();

    // 获取当前任务ID
    let current_tid = task.inner_exclusive_access().res.as_ref().unwrap().tid;

    // 构建所需数据结构
    let tasks: Vec<_> = process_inner
        .tasks
        .iter()
        .enumerate()
        .filter_map(
            |(i, task_opt)| {
                if task_opt.is_some() {
                    Some(i)
                } else {
                    None
                }
            },
        )
        .collect();
    let n = tasks.len();
    if n == 0 {
        return false;
    }

    let m = process_inner.mutex_list.len();
    if m == 0 {
        return false;
    }

    // 构建Available向量 - 互斥锁只有0或1两种状态
    let mut available = vec![1; m];
    for (mutex_id, holder) in process_inner.mutex_holders.iter().enumerate() {
        if holder.is_some() {
            available[mutex_id] = 0; // 已被占用
        }
    }

    // 设置Work = Available
    let mut work = available.clone();

    // 设置Finish向量
    let mut finish = vec![false; n];

    // 假设当前任务等待指定的mutex_id
    let mut allocation: Vec<Vec<usize>> = vec![vec![0; m]; n];
    let mut request: Vec<Vec<usize>> = vec![vec![0; m]; n];

    // 填充Allocation矩阵
    for (mutex_id, holder) in process_inner.mutex_holders.iter().enumerate() {
        if let Some(task_id) = holder {
            if let Some(task_index) = tasks.iter().position(|&id| id == *task_id) {
                allocation[task_index][mutex_id] = 1;
            }
        }
    }

    // 构建Request矩阵
    // 每个任务只需要一个锁实例
    for (task_index, &task_id) in tasks.iter().enumerate() {
        let task_opt = &process_inner.tasks[task_id];
        if let Some(task) = task_opt {
            let task_inner = task.inner_exclusive_access();
            if let Some(waiting_mutex) = task_inner.waiting_for_mutex {
                // 记录该任务正在等待的资源
                request[task_index][waiting_mutex] = 1;
            }
        }
    }

    // 假设当前任务要请求mutex_id
    if let Some(current_index) = tasks.iter().position(|&id| id == current_tid) {
        request[current_index][mutex_id] = 1;
    }

    // 安全性检查算法
    loop {
        // 查找符合条件的进程
        let mut found = false;
        for i in 0..n {
            if finish[i] {
                continue;
            } // 已完成的跳过

            // 检查是否所有请求都能满足
            let mut can_satisfy = true;
            for j in 0..m {
                if request[i][j] > work[j] {
                    can_satisfy = false;
                    break;
                }
            }

            if can_satisfy {
                // 假设分配并释放资源
                for j in 0..m {
                    work[j] += allocation[i][j];
                }
                finish[i] = true;
                found = true;
            }
        }

        if !found {
            break; // 未找到可以满足的进程，退出循环
        }
    }

    // 检查是否所有进程都能完成
    for f in finish {
        if !f {
            return true; // 检测到死锁
        }
    }

    false // 安全状态，无死锁
}

// 检测信号量死锁
fn detect_semaphore_deadlock(sem_id: usize) -> bool {
    let task = current_task().unwrap();
    let process = task.process.upgrade().unwrap();
    let process_inner = process.inner_exclusive_access();

    // 获取当前任务ID (在 process_inner.tasks 中的索引)
    let current_tid = task.inner_exclusive_access().res.as_ref().unwrap().tid;

    // 构建活动任务列表 (tasks[i] 是 process_inner.tasks 中的实际索引/tid)
    let tasks: Vec<_> = process_inner
        .tasks
        .iter()
        .enumerate()
        .filter_map(|(tid, task_opt)| if task_opt.is_some() { Some(tid) } else { None })
        .collect();
    let n = tasks.len(); // 活动任务数量
    if n == 0 {
        return false;
    }

    let m = process_inner.semaphore_list.len(); // 信号量类型数量
    if m == 0 || sem_id >= m {
        return false;
    }

    // 构建Available向量 (使用 semaphore_initial_counts)
    let mut available = vec![0; m];
    for sem_idx in 0..m {
        let initial_count = process_inner
            .semaphore_initial_counts
            .get(sem_idx)
            .copied()
            .unwrap_or(0);

        let mut allocated: usize = 0;
        if let Some(allocs) = process_inner.semaphore_allocations.get(sem_idx) {
            // 正确计算总分配量：遍历 allocs Vec<usize>
            // 假设 allocs 的索引是 tid
            allocated = allocs.iter().sum();
        }
        available[sem_idx] = initial_count.saturating_sub(allocated);
    }

    // 设置Work = Available
    let mut work = available.clone();
    // 设置Finish向量
    let mut finish = vec![false; n];

    // 构建Allocation矩阵 (n x m)
    // allocation[i][j] 表示活动任务 i (对应 process_inner.tasks[tasks[i]]) 持有的信号量 j 的数量
    let mut allocation = vec![vec![0; m]; n];
    for i in 0..n {
        // 遍历活动任务索引
        let task_tid = tasks[i]; // 获取该活动任务在 process_inner.tasks 中的实际 tid
        for j in 0..m {
            // 遍历信号量类型
            if let Some(allocs) = process_inner.semaphore_allocations.get(j) {
                // 从 allocs 中获取 task_tid 的分配数量
                allocation[i][j] = allocs.get(task_tid).copied().unwrap_or(0);
            }
        }
    }

    // 构建Request矩阵 (n x m)
    let mut request = vec![vec![0; m]; n];
    for i in 0..n {
        // 遍历活动任务索引
        let task_tid = tasks[i]; // 获取实际 tid
                                 // 访问对应的 TaskControlBlock
        if let Some(task_arc) = &process_inner.tasks[task_tid] {
            let task_inner = task_arc.inner_exclusive_access();
            if let Some(waiting_sem) = task_inner.waiting_for_semaphore {
                request[i][waiting_sem] = 1; // 假设请求量为 1
            }
        }
    }

    // 假设当前任务要请求sem_id
    // 找到当前任务在活动任务列表 tasks 中的索引 i
    if let Some(current_index) = tasks.iter().position(|&tid| tid == current_tid) {
        request[current_index][sem_id] = 1; // 假设请求量为 1
    }

    // --- 安全性检查算法 (保持不变) ---
    loop {
        let mut found = false;
        for i in 0..n {
            if finish[i] {
                continue;
            }
            let mut can_satisfy = true;
            for j in 0..m {
                if request[i][j] > work[j] {
                    can_satisfy = false;
                    break;
                }
            }
            if can_satisfy {
                for j in 0..m {
                    work[j] += allocation[i][j];
                }
                finish[i] = true;
                found = true;
            }
        }
        if !found {
            break;
        }
    }

    // 检查是否所有进程都能完成
    !finish.iter().all(|&f| f)
}

/// sleep syscall
pub fn sys_sleep(ms: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_sleep",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}
/// mutex create syscall
pub fn sys_mutex_create(blocking: bool) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        process_inner.mutex_list.len() as isize - 1
    }
}
/// mutex lock syscall
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_lock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );

    // 获取当前任务和进程
    let task = current_task().unwrap();
    let process = current_process();

    // 检查死锁检测功能是否启用
    let deadlock_detect = process.inner_exclusive_access().dead_lock_detect_enabled;

    // 检查mutex_id是否有效
    let process_inner = process.inner_exclusive_access();
    if mutex_id >= process_inner.mutex_list.len() || process_inner.mutex_list[mutex_id].is_none() {
        return -1;
    }
    // 获取互斥锁对象，在释放 process_inner 之前克隆引用
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());

    // 如果需要，检测死锁
    if deadlock_detect {
        // 检查是否需要扩展 mutex_holders 数组
        let needs_resize = mutex_id >= process_inner.mutex_holders.len();

        // 查看互斥锁是否已被持有
        let is_locked = if mutex_id < process_inner.mutex_holders.len() {
            process_inner.mutex_holders[mutex_id].is_some()
        } else {
            false
        };

        // 释放进程锁，以便稍后可能需要进行的操作
        drop(process_inner);

        // 如果需要扩展数组
        if needs_resize {
            let mut process_inner = process.inner_exclusive_access();
            process_inner.mutex_holders.resize(mutex_id + 1, None);
            drop(process_inner);
        }

        // 如果互斥锁已被持有，检测是否会导致死锁
        if is_locked {
            // 检测死锁
            if detect_mutex_deadlock(mutex_id) {
                return DEADLOCK_ERROR; // 检测到死锁，拒绝请求
            }
        }
    } else {
        // 如果不需要检测死锁，直接释放进程锁
        drop(process_inner);
    }

    // 记录请求信息
    let tid = task.inner_exclusive_access().res.as_ref().unwrap().tid;
    task.inner_exclusive_access().waiting_for_mutex = Some(mutex_id);

    // 尝试获取锁
    mutex.lock();

    // 锁获取成功，更新持有者信息
    let mut process_inner = process.inner_exclusive_access();
    // 确保mutex_holders数组大小足够
    if mutex_id >= process_inner.mutex_holders.len() {
        process_inner.mutex_holders.resize(mutex_id + 1, None);
    }
    process_inner.mutex_holders[mutex_id] = Some(tid);

    // 清除等待信息
    task.inner_exclusive_access().waiting_for_mutex = None;

    0
}

/// mutex unlock syscall
pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_unlock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );

    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();

    // 检查mutex_id是否有效
    if mutex_id >= process_inner.mutex_list.len() || process_inner.mutex_list[mutex_id].is_none() {
        return -1;
    }

    // 获取互斥锁对象
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());

    // 更新持有者信息（在解锁前）
    if mutex_id < process_inner.mutex_holders.len() {
        process_inner.mutex_holders[mutex_id] = None;
    }

    drop(process_inner);

    // 解锁
    mutex.unlock();

    0
}
/// semaphore create syscall
pub fn sys_semaphore_create(res_count: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    println!("Semaphore created:{}",res_count);
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));

        // 确保初始计数数组大小足够 (使用 semaphore_initial_counts)
        if id >= process_inner.semaphore_initial_counts.len() {
            process_inner.semaphore_initial_counts.resize(id + 1, 0);
        }
        process_inner.semaphore_initial_counts[id] = res_count; // 存储初始计数

        // 确保 semaphore_allocations 数组大小足够并初始化为空 Vec
        if id >= process_inner.semaphore_allocations.len() {
            process_inner
                .semaphore_allocations
                .resize_with(id + 1, Vec::new);
        }
        // 清空可能存在的旧数据（如果重用ID）
        process_inner.semaphore_allocations[id].clear();

        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));

        // 添加初始计数 (使用 semaphore_initial_counts)
        process_inner.semaphore_initial_counts.push(res_count);
        // 添加空的 allocation vector
        process_inner.semaphore_allocations.push(Vec::new());

        process_inner.semaphore_list.len() - 1
    };

    id as isize
}
/// semaphore up syscall
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_up",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );

    let process = current_process();
    let task = current_task().unwrap();
    let tid = task.inner_exclusive_access().res.as_ref().unwrap().tid;

    let mut process_inner = process.inner_exclusive_access();

    // 检查sem_id是否有效
    if sem_id >= process_inner.semaphore_list.len()
        || process_inner.semaphore_list[sem_id].is_none()
    {
        return -1;
    }

    // 获取信号量对象
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());

    // 更新分配信息
    if sem_id < process_inner.semaphore_allocations.len()
        && tid < process_inner.semaphore_allocations[sem_id].len()
    {
        if process_inner.semaphore_allocations[sem_id][tid] > 0 {
            process_inner.semaphore_allocations[sem_id][tid] -= 1;
        }
    }

    drop(process_inner);

    // 释放信号量
    sem.up();

    0
}
/// semaphore down syscall
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_down",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );

    // 获取当前任务和进程
    let task = current_task().unwrap();
    let process = current_process();

    // 检查死锁检测功能是否启用
    let deadlock_detect = process.inner_exclusive_access().dead_lock_detect_enabled;

    // 检查sem_id是否有效
    let mut process_inner = process.inner_exclusive_access();
    if sem_id >= process_inner.semaphore_list.len()
        || process_inner.semaphore_list[sem_id].is_none()
    {
        return -1;
    }

    // 如果需要，进行死锁检测
    if deadlock_detect {
        println!("Deadlock detect!!");
        // 确保所需数据结构已初始化，使用初始计数而非当前计数
        if sem_id >= process_inner.semaphore_initial_counts.len() {
            // 这个信号量创建时没有正确记录初始计数，使用默认值0（安全但可能不准确）
            process_inner.semaphore_initial_counts.resize(sem_id + 1, 0);
            // 注意：这里理想情况是找到创建时的初始值，但无法直接获取
        }

        if sem_id >= process_inner.semaphore_allocations.len() {
            process_inner
                .semaphore_allocations
                .resize(sem_id + 1, Vec::new());
        }

        let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
        let sem_inner = sem.inner.exclusive_access();
        println!("Sem_id {}'s count={}",sem_id,sem_inner.count);
        // 如果信号量当前值为0，检测是否会导致死锁
        if sem_inner.count <= 0 {
            // 暂时释放锁以避免死锁
            drop(sem_inner);
            drop(process_inner);

            // 检测死锁
            println!("I am going to detect dead lock");
            if detect_semaphore_deadlock(sem_id) {
                return DEADLOCK_ERROR; // 检测到死锁，拒绝请求
            }

            // 重新获取process_inner
            process_inner = process.inner_exclusive_access();
        } else {
            drop(sem_inner);
        }
    }

    // 获取信号量对象
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());

    // 记录请求信息
    let tid = task.inner_exclusive_access().res.as_ref().unwrap().tid;
    task.inner_exclusive_access().waiting_for_semaphore = Some(sem_id);

    // 释放进程锁，避免死锁
    drop(process_inner);

    // 尝试申请信号量
    sem.down();

    // 信号量取成功，更新分配信息
    let mut process_inner = process.inner_exclusive_access();

    // 确保所需数据结构已初始化
    if sem_id >= process_inner.semaphore_allocations.len() {
        process_inner
            .semaphore_allocations
            .resize(sem_id + 1, Vec::new());
    }

    let alloc = &mut process_inner.semaphore_allocations[sem_id];
    if tid >= alloc.len() {
        alloc.resize(tid + 1, 0);
    }
    alloc[tid] += 1;

    // 清除等待信息
    task.inner_exclusive_access().waiting_for_semaphore = None;

    0
}
/// condvar create syscall
pub fn sys_condvar_create() -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}
/// condvar signal syscall
pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_signal",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}
/// condvar wait syscall
pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_wait",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    condvar.wait(mutex);
    0
}
/// enable deadlock detection syscall
///
/// YOUR JOB: Implement deadlock detection, but might not all in this syscall
pub fn sys_enable_deadlock_detect(enabled: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_enable_deadlock_detect",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );

    // 获取当前任务
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();

    // 根据参数设置死锁检测状态
    match enabled {
        0 => {
            process_inner.dead_lock_detect_enabled = false;
            0
        }
        1 => {
            process_inner.dead_lock_detect_enabled = true;
            0
        }
        _ => -1, // 参数不合法
    }
}
