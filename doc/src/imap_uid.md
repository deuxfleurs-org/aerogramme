# IMAP UID proof

**Notations**

- $h$: the hash of a message, $\mathbb{H}$ is the set of hashes
- $i$: the UID of a message $(i \in \mathbb{N})$
- $f$: a flag attributed to a message (it's a string), we write
  $\mathbb{F}$ the set of possible flags
- if $M$ is a map (aka a dictionnary), if $x$ has no assigned value in
  $M$ we write $M [x] = \bot$ or equivalently $x \not\in M$. If $x$ has a value
  in the map we write $x \in M$ and $M [x] \neq \bot$

**State**

- A map $I$ such that $I [h]$ is the UID of the message whose hash is
  $h$ is the mailbox, or $\bot$ if there is no such message

- A map $F$ such that $F [h]$ is the set of flags attributed to the
  message whose hash is $h$

- $v$: the UIDVALIDITY value

- $n$: the UIDNEXT value

- $s$: an internal sequence number that is mostly equal to UIDNEXT but
  also grows when mails are deleted
  
**Operations**

  - MAIL\_ADD$(h, i)$: the value of $i$ that is put in this operation is
  the value of $s$ in the state resulting of all already known operations,
  i.e. $s (O_{gen})$ in the notation below where $O_{gen}$ is
  the set of all operations known at the time when the MAIL\_ADD is generated.
  Moreover, such an operation can only be generated if $I (O_{gen}) [h]
  = \bot$, i.e. for a mail $h$ that is not already in the state at
  $O_{gen}$.

  - MAIL\_DEL$(h)$

  - FLAG\_ADD$(h, f)$

  - FLAG\_DEL$(h, f)$

**Algorithms**


**apply** MAIL\_ADD$(h, i)$:  
&nbsp;&nbsp; *if* $i < s$:  
&nbsp;&nbsp;&nbsp;&nbsp; $v \leftarrow v + s - i$  
&nbsp;&nbsp; *if* $F [h] = \bot$:  
&nbsp;&nbsp;&nbsp;&nbsp; $F [h] \leftarrow F_{initial}$  
&nbsp;&nbsp;$I [h] \leftarrow s$  
&nbsp;&nbsp;$s \leftarrow s + 1$  
&nbsp;&nbsp;$n \leftarrow s$  

**apply** MAIL\_DEL$(h)$:  
&nbsp;&nbsp; $I [h] \leftarrow \bot$  
&nbsp;&nbsp;$F [h] \leftarrow \bot$  
&nbsp;&nbsp;$s \leftarrow s + 1$

**apply** FLAG\_ADD$(h, f)$:  
&nbsp;&nbsp; *if* $h \in F$:  
&nbsp;&nbsp;&nbsp;&nbsp; $F [h] \leftarrow F [h] \cup \{ f \}$  

**apply** FLAG\_DEL$(h, f)$:  
&nbsp;&nbsp; *if* $h \in F$:  
&nbsp;&nbsp;&nbsp;&nbsp; $F [h] \leftarrow F [h] \backslash \{ f \}$  


**More notations**

- $o$ is an operation such as MAIL\_ADD, MAIL\_DEL, etc. $O$ is a set of
  operations. Operations embed a timestamp, so a set of operations $O$ can be
  written as $O = [o_1, o_2, \ldots, o_n]$ by ordering them by timestamp.

- if $o \in O$, we write $O_{\leqslant o}$, $O_{< o}$, $O_{\geqslant
  o}$, $O_{> o}$ the set of items of $O$ that are respectively earlier or
  equal, strictly earlier, later or equal, or strictly later than $o$. In
  other words, if we write $O = [o_1, \ldots, o_n]$, where $o$ is a certain
  $o_i$ in this sequence, then:
$$
\begin{aligned}
O_{\leqslant o} &=  \{ o_1, \ldots, o_i \}\\
O_{< o} &= \{ o_1, \ldots, o_{i - 1} \}\\
O_{\geqslant o} &= \{ o_i, \ldots, o_n \}\\
O_{> o} &= \{ o_{i + 1}, \ldots, o_n \}
\end{aligned}
$$

- If $O$ is a set of operations, we write $I (O)$, $F (O)$, $n (O), s
  (O)$, and $v (O)$ the values of $I, F, n, s$ and $v$ in the state that
  results of applying all of the operations in $O$ in their sorted order. (we
  thus write $I (O) [h]$ the value of $I [h]$ in this state)

**Hypothesis:** 
An operation $o$ can only be in a set $O$ if it was
generated after applying operations of a set $O_{gen}$ such that
$O_{gen} \subset O$ (because causality is respected in how we deliver
operations). Sets of operations that do not respect this property are excluded
from all of the properties, lemmas and proofs below.

**Simplification:** We will now exclude FLAG\_ADD and FLAG\_DEL
operations, as they do not manipulate $n$, $s$ and $v$, and adding them should
have no impact on the properties below.

**Small lemma:** If there are no FLAG\_ADD and FLAG\_DEL operations,
then $s (O) = | O |$. This is easy to see because the possible operations are
only MAIL\_ADD and MAIL\_DEL, and both increment the value of $s$ by 1.

**Defnition:** If $o$ is a MAIL\_ADD$(h, i)$ operation, and $O$ is a
set of operations such that $o \in O$, then we define the following value:
$$
C (o, O) = s (O_{< o}) - i
$$
We say that $C (o, O)$ is the *number of conflicts of $o$ in $O$*: it
corresponds to the number of operations that were added before $o$ in $O$ that
were not in $O_{gen}$.

**Property:**

We have that:

$$
v (O) = \sum_{o \in O} C (o, O)
$$

Or in English: $v (O)$ is the sum of the number of conflicts of all of the
MAIL\_ADD operations in $O$. This is easy to see because indeed $v$ is
incremented by $C (o, O)$ for each operation $o \in O$ that is applied.


**Property:**
  If $O$ and $O'$ are two sets of operations, and $O \subseteq O'$, then:

$$
\begin{aligned}
\forall o \in O, \qquad C (o, O) \leqslant C (o, O')
\end{aligned}
$$

This is easy to see because $O_{< o} \subseteq O'_{< o}$ and $C (o, O') - C
  (o, O) = s (O'_{< o}) - s (O_{< o}) = | O'_{< o} | - | O_{< o} | \geqslant
  0$

**Theorem:**

If $O$ and $O'$ are two sets of operations:

$$
\begin{aligned}
O \subseteq O' & \Rightarrow & v (O) \leqslant v (O')
\end{aligned}
$$

**Proof:** 

$$
\begin{aligned}
v (O') &= \sum_{o \in O'} C (o, O')\\
  & \geqslant \sum_{o \in O} C (o, O') \qquad \text{(because $O \subseteq
  O'$)}\\
  & \geqslant \sum_{o \in O} C (o, O) \qquad \text{(because $\forall o \in
  O, C (o, O) \leqslant C (o, O')$)}\\
  & \geqslant v (O)
\end{aligned}
$$

**Theorem:**

If $O$ and $O'$ are two sets of operations, such that $O \subset O'$,

and if there are two different mails $h$ and $h'$ $(h \neq h')$ such that $I
  (O) [h] = I (O') [h']$

  then:
  $$v (O) < v (O')$$

**Proof:**

We already know that $v (O) \leqslant v (O')$ because of the previous theorem.
We will now look at the sum:
$$
v (O') = \sum_{o \in O'} C (o, O')
$$
and show that there is at least one term in this sum that is strictly larger
than the corresponding term in the other sum:
$$
v (O) = \sum_{o \in O} C (o, O)
$$
Let $o$ be the last MAIL\_ADD$(h, \_)$ operation in $O$, i.e. the operation
that gives its definitive UID to mail $h$ in $O$, and similarly $o'$ be the
last MAIL\_ADD($h', \_$) operation in $O'$.

Let us write $I = I (O) [h] = I (O') [h']$

$o$ is the operation at position $I$ in $O$, and $o'$ is the operation at
position $I$ in $O'$. But $o \neq o'$, so if $o$ is not the operation at
position $I$ in $O'$ then it has to be at a later position $I' > I$ in $O'$,
because no operations are removed between $O$ and $O'$, the only possibility
is that some other operations (including $o'$) are added before $o$. Therefore
we have that $C (o, O') > C (o, O)$, i.e. at least one term in the sum above
is strictly larger in the first sum than in the second one. Since all other
terms are greater or equal, we have $v (O') > v (O)$.
