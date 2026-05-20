# 两页空白PPT的文字草稿大纲

## 1
介绍PAVE和HIST
- PAVE：Parameter Vector. 目标样本上微调后，一个选定模型参数空间的改变（位移）。
- HIST：Hidden State Vector. 神经网络在处理某个输入时，于特定层、特定位置生成的内部表示向量，用来编码该输入在当前上下文中的语义、结构和任务相关信息。

PAVE因为需要对一个模型进行训练，适合基于一定数量上的数据集上刻画目标模型的能力。而HIST作为任务表征不能表示模型能力，但是可以对少量（一条）数据进行描述。

### 公式
PAVE：$ \tau = argmin\  loss(h(x|\theta_0+\tau),\ f(x)) $
HIST: $ h^{\mathrm{small}}_{L_s,T}(x) := \big[H^{\mathrm{small}}_{L_s}(x)\big]_{T}\in\mathbb{R}^{d_s} $

## 2
PAVE如何向HIST转化以实现model和query的比较

PAVE位于参数空间，不能直接与query的HIST比较。  
因此，对候选模型 \(m\)，可以观察其参数位移 \(\tau_m\) 作用后，在同一query上产生的hidden state，并与base model的hidden state进行比较。

给定base model参数 \(\theta_0\)，候选模型 \(m\) 的PAVE为：

\[
\tau_m=\theta_m-\theta_0
\]

在query \(x\) 上，模型 \(m\) 的hidden state表示为：

\[
h_{\ell,T}(x;\theta_0+\tau_m)
\]

base model的hidden state表示为：

\[
h_{\ell,T}(x;\theta_0)
\]

二者的相似度可定义为：

\[
s(m,x)=
\cos\left(
h_{\ell,T}(x;\theta_0+\tau_m),
h_{\ell,T}(x;\theta_0)
\right)
\]

其中，\(\ell\) 表示层，\(T\) 表示最后一个输入token。

该分数衡量的是：候选模型在处理query \(x\) 时，其内部表示与base model表示的对齐程度。  
分数越高，说明该模型在当前query上的表示偏移越小；分数越低，说明PAVE对该query的内部表示产生了更强的改变。

因此，这一方法可以将参数空间中的PAVE，转化为query条件下的hidden-state表示比较，用于model selection。