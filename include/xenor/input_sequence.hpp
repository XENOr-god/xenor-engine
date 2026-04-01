#pragma once

#include <algorithm>
#include <cstddef>
#include <initializer_list>
#include <stdexcept>
#include <utility>
#include <vector>

namespace xenor {

template <typename Input>
class InputSequence {
public:
  using value_type = Input;
  using container_type = std::vector<Input>;
  using size_type = typename container_type::size_type;
  static constexpr size_type npos = static_cast<size_type>(-1);

  InputSequence() = default;
  InputSequence(std::initializer_list<Input> inputs) : inputs_(inputs) {}
  explicit InputSequence(container_type inputs) : inputs_(std::move(inputs)) {}

  [[nodiscard]] bool empty() const noexcept { return inputs_.empty(); }
  [[nodiscard]] size_type size() const noexcept { return inputs_.size(); }

  [[nodiscard]] const Input& operator[](size_type index) const { return inputs_.at(index); }
  [[nodiscard]] Input& operator[](size_type index) { return inputs_.at(index); }

  void push_back(Input input) { inputs_.push_back(std::move(input)); }

  [[nodiscard]] const Input* data() const noexcept { return inputs_.data(); }
  [[nodiscard]] auto begin() const noexcept { return inputs_.begin(); }
  [[nodiscard]] auto end() const noexcept { return inputs_.end(); }

  [[nodiscard]] InputSequence slice(size_type offset,
                                    size_type count = npos) const {
    if (offset > inputs_.size()) {
      throw std::out_of_range("input sequence slice offset is out of range");
    }

    const auto available = inputs_.size() - offset;
    const auto slice_count = std::min(count, available);

    return InputSequence(container_type{
        inputs_.begin() + static_cast<std::ptrdiff_t>(offset),
        inputs_.begin() + static_cast<std::ptrdiff_t>(offset + slice_count)});
  }

private:
  container_type inputs_;
};

}  // namespace xenor
