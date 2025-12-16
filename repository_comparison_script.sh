#!/bin/bash
# repository_comparison_script.sh
# Automated script for comparing op-dbus-v2-old and op-dbus-v2 repositories

set -e

OLD_REPO="op-dbus-v2-old"
NEW_REPO="op-dbus-v2"
OUTPUT_DIR="comparison_results"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")

echo "🔍 Starting Repository Comparison Analysis"
echo "=========================================="
echo "Timestamp: $TIMESTAMP"
echo "Old Repository: $OLD_REPO"
echo "New Repository: $NEW_REPO"
echo ""

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Function to check if repository exists
check_repo() {
    local repo=$1
    if [ ! -d "$repo" ]; then
        echo "❌ Error: Repository '$repo' not found!"
        echo "Please ensure both repositories are cloned in the current directory."
        exit 1
    fi
    echo "✅ Found repository: $repo"
}

# Check repositories
echo "📁 Checking repositories..."
check_repo "$OLD_REPO"
check_repo "$NEW_REPO"
echo ""

# 1. Directory Structure Comparison
echo "📂 Analyzing directory structure..."
echo "=== Directory Structure Analysis ===" > "$OUTPUT_DIR/structure_analysis_$TIMESTAMP.txt"

echo "Old repository structure:" >> "$OUTPUT_DIR/structure_analysis_$TIMESTAMP.txt"
find "$OLD_REPO" -type f | head -50 >> "$OUTPUT_DIR/structure_analysis_$TIMESTAMP.txt"
echo "" >> "$OUTPUT_DIR/structure_analysis_$TIMESTAMP.txt"

echo "New repository structure:" >> "$OUTPUT_DIR/structure_analysis_$TIMESTAMP.txt"
find "$NEW_REPO" -type f | head -50 >> "$OUTPUT_DIR/structure_analysis_$TIMESTAMP.txt"
echo "" >> "$OUTPUT_DIR/structure_analysis_$TIMESTAMP.txt"

# File count comparison
echo "=== File Count Comparison ===" >> "$OUTPUT_DIR/structure_analysis_$TIMESTAMP.txt"
echo "Old repo (.js files): $(find "$OLD_REPO" -name "*.js" | wc -l)" >> "$OUTPUT_DIR/structure_analysis_$TIMESTAMP.txt"
echo "New repo (.ts files): $(find "$NEW_REPO" -name "*.ts" | wc -l)" >> "$OUTPUT_DIR/structure_analysis_$TIMESTAMP.txt"
echo "Old repo (.json files): $(find "$OLD_REPO" -name "*.json" | wc -l)" >> "$OUTPUT_DIR/structure_analysis_$TIMESTAMP.txt"
echo "New repo (.json files): $(find "$NEW_REPO" -name "*.json" | wc -l)" >> "$OUTPUT_DIR/structure_analysis_$TIMESTAMP.txt"

echo "✅ Directory structure analysis saved to: $OUTPUT_DIR/structure_analysis_$TIMESTAMP.txt"
echo ""

# 2. Missing Files Analysis
echo "🔍 Finding missing files..."
echo "=== Missing Files Analysis ===" > "$OUTPUT_DIR/missing_files_$TIMESTAMP.txt"

# Get list of all files in both repos
find "$OLD_REPO" -type f | sed "s|$OLD_REPO/||" | sort > "$OUTPUT_DIR/old_files_$TIMESTAMP.txt"
find "$NEW_REPO" -type f | sed "s|$NEW_REPO/||" | sort > "$OUTPUT_DIR/new_files_$TIMESTAMP.txt"

# Find missing files (files in old but not in new)
echo "Files in old repo but missing in new repo:" >> "$OUTPUT_DIR/missing_files_$TIMESTAMP.txt"
comm -23 "$OUTPUT_DIR/old_files_$TIMESTAMP.txt" "$OUTPUT_DIR/new_files_$TIMESTAMP.txt" >> "$OUTPUT_DIR/missing_files_$TIMESTAMP.txt"

# Find new files (files in new but not in old)
echo "" >> "$OUTPUT_DIR/missing_files_$TIMESTAMP.txt"
echo "New files in refactored repo:" >> "$OUTPUT_DIR/missing_files_$TIMESTAMP.txt"
comm -13 "$OUTPUT_DIR/old_files_$TIMESTAMP.txt" "$OUTPUT_DIR/new_files_$TIMESTAMP.txt" >> "$OUTPUT_DIR/missing_files_$TIMESTAMP.txt"

echo "✅ Missing files analysis saved to: $OUTPUT_DIR/missing_files_$TIMESTAMP.txt"
echo ""

# 3. Function/Class Analysis
echo "🔧 Analyzing functions and classes..."
echo "=== Function and Class Analysis ===" > "$OUTPUT_DIR/functions_analysis_$TIMESTAMP.txt"

# Extract functions from old repo
echo "Functions in old repository:" >> "$OUTPUT_DIR/functions_analysis_$TIMESTAMP.txt"
find "$OLD_REPO" -name "*.js" -exec grep -Hn "function\|const.*=.*function\|class\|const.*=" {} \; >> "$OUTPUT_DIR/functions_analysis_$TIMESTAMP.txt" 2>/dev/null || true

echo "" >> "$OUTPUT_DIR/functions_analysis_$TIMESTAMP.txt"
echo "Functions/classes in new repository:" >> "$OUTPUT_DIR/functions_analysis_$TIMESTAMP.txt"
find "$NEW_REPO" -name "*.ts" -exec grep -Hn "function\|const.*=.*function\|class\|const.*=" {} \; >> "$OUTPUT_DIR/functions_analysis_$TIMESTAMP.txt" 2>/dev/null || true

echo "✅ Functions analysis saved to: $OUTPUT_DIR/functions_analysis_$TIMESTAMP.txt"
echo ""

# 4. Configuration Comparison
echo "⚙️ Comparing configuration files..."
echo "=== Configuration Comparison ===" > "$OUTPUT_DIR/config_comparison_$TIMESTAMP.txt"

if [ -f "$OLD_REPO/package.json" ]; then
    echo "Old repository dependencies:" >> "$OUTPUT_DIR/config_comparison_$TIMESTAMP.txt"
    grep -A 20 '"dependencies"' "$OLD_REPO/package.json" >> "$OUTPUT_DIR/config_comparison_$TIMESTAMP.txt" 2>/dev/null || echo "No dependencies found" >> "$OUTPUT_DIR/config_comparison_$TIMESTAMP.txt
fi

echo "" >> "$OUTPUT_DIR/config_comparison_$TIMESTAMP.txt"

if [ -f "$NEW_REPO/package.json" ]; then
    echo "New repository dependencies:" >> "$OUTPUT_DIR/config_comparison_$TIMESTAMP.txt"
    grep -A 20 '"dependencies"' "$NEW_REPO/package.json" >> "$OUTPUT_DIR/config_comparison_$TIMESTAMP.txt" 2>/dev/null || echo "No dependencies found" >> "$OUTPUT_DIR/config_comparison_$TIMESTAMP.txt"
fi

echo "✅ Configuration comparison saved to: $OUTPUT_DIR/config_comparison_$TIMESTAMP.txt"
echo ""

# 5. TODO/FIXME Analysis
echo "📝 Searching for TODO and FIXME comments..."
echo "=== TODO/FIXME Analysis ===" > "$OUTPUT_DIR/todos_analysis_$TIMESTAMP.txt"

echo "TODO/FIXME in old repository:" >> "$OUTPUT_DIR/todos_analysis_$TIMESTAMP.txt"
find "$OLD_REPO" -name "*.js" -exec grep -Hn "TODO\|FIXME\|XXX\|HACK" {} \; >> "$OUTPUT_DIR/todos_analysis_$TIMESTAMP.txt" 2>/dev/null || echo "No TODO/FIXME found" >> "$OUTPUT_DIR/todos_analysis_$TIMESTAMP.txt"

echo "" >> "$OUTPUT_DIR/todos_analysis_$TIMESTAMP.txt"

echo "TODO/FIXME in new repository:" >> "$OUTPUT_DIR/todos_analysis_$TIMESTAMP.txt"
find "$NEW_REPO" -name "*.ts" -exec grep -Hn "TODO\|FIXME\|XXX\|HACK" {} \; >> "$OUTPUT_DIR/todos_analysis_$TIMESTAMP.txt" 2>/dev/null || echo "No TODO/FIXME found" >> "$OUTPUT_DIR/todos_analysis_$TIMESTAMP.txt"

echo "✅ TODO/FIXME analysis saved to: $OUTPUT_DIR/todos_analysis_$TIMESTAMP.txt"
echo ""

# 6. Generate Summary Report
echo "📊 Generating summary report..."
echo "=== Repository Comparison Summary ===" > "$OUTPUT_DIR/summary_$TIMESTAMP.txt"
echo "Analysis Date: $(date)" >> "$OUTPUT_DIR/summary_$TIMESTAMP.txt"
echo "" >> "$OUTPUT_DIR/summary_$TIMESTAMP.txt"

echo "File Statistics:" >> "$OUTPUT_DIR/summary_$TIMESTAMP.txt"
echo "- Old repository total files: $(find "$OLD_REPO" -type f | wc -l)" >> "$OUTPUT_DIR/summary_$TIMESTAMP.txt"
echo "- New repository total files: $(find "$NEW_REPO" -type f | wc -l)" >> "$OUTPUT_DIR/summary_$TIMESTAMP.txt"
echo "- Missing files: $(comm -23 "$OUTPUT_DIR/old_files_$TIMESTAMP.txt" "$OUTPUT_DIR/new_files_$TIMESTAMP.txt" | wc -l)" >> "$OUTPUT_DIR/summary_$TIMESTAMP.txt"
echo "- New files: $(comm -13 "$OUTPUT_DIR/old_files_$TIMESTAMP.txt" "$OUTPUT_DIR/new_files_$TIMESTAMP.txt" | wc -l)" >> "$OUTPUT_DIR/summary_$TIMESTAMP.txt"

echo "" >> "$OUTPUT_DIR/summary_$TIMESTAMP.txt"
echo "Next Steps:" >> "$OUTPUT_DIR/summary_$TIMESTAMP.txt"
echo "1. Review missing_files_$TIMESTAMP.txt for functionality to port" >> "$OUTPUT_DIR/summary_$TIMESTAMP.txt"
echo "2. Check functions_analysis_$TIMESTAMP.txt for missing implementations" >> "$OUTPUT_DIR/summary_$TIMESTAMP.txt"
echo "3. Compare configurations for dependency changes" >> "$OUTPUT_DIR/summary_$TIMESTAMP.txt"
echo "4. Address TODO/FIXME items identified" >> "$OUTPUT_DIR/summary_$TIMESTAMP.txt"

echo "✅ Summary report saved to: $OUTPUT_DIR/summary_$TIMESTAMP.txt"
echo ""

# 7. Create prioritized action items
echo "🎯 Creating action items..."
echo "=== Priority Action Items ===" > "$OUTPUT_DIR/action_items_$TIMESTAMP.txt"
echo "" >> "$OUTPUT_DIR/action_items_$TIMESTAMP.txt"

echo "HIGH PRIORITY (Critical Missing Functionality):" >> "$OUTPUT_DIR/action_items_$TIMESTAMP.txt"
echo "- Review missing core D-Bus communication files" >> "$OUTPUT_DIR/action_items_$TIMESTAMP.txt"
echo "- Identify missing authentication mechanisms" >> "$OUTPUT_DIR/action_items_$TIMESTAMP.txt"
echo "- Check for missing error handling implementations" >> "$OUTPUT_DIR/action_items_$TIMESTAMP.txt"
echo "" >> "$OUTPUT_DIR/action_items_$TIMESTAMP.txt"

echo "MEDIUM PRIORITY (Important Features):" >> "$OUTPUT_DIR/action_items_$TIMESTAMP.txt"
echo "- Review event system implementations" >> "$OUTPUT_DIR/action_items_$TIMESTAMP.txt"
echo "- Check configuration management" >> "$OUTPUT_DIR/action_items_$TIMESTAMP.txt"
echo "- Validate logging and monitoring" >> "$OUTPUT_DIR/action_items_$TIMESTAMP.txt"
echo "" >> "$OUTPUT_DIR/action_items_$TIMESTAMP.txt"

echo "LOW PRIORITY (Enhancements):" >> "$OUTPUT_DIR/action_items_$TIMESTAMP.txt"
echo "- Performance optimizations" >> "$OUTPUT_DIR/action_items_$TIMESTAMP.txt"
echo "- Documentation improvements" >> "$OUTPUT_DIR/action_items_$TIMESTAMP.txt"
echo "- Code quality enhancements" >> "$OUTPUT_DIR/action_items_$TIMESTAMP.txt"

echo "✅ Action items saved to: $OUTPUT_DIR/action_items_$TIMESTAMP.txt"
echo ""

echo "🎉 Repository comparison analysis complete!"
echo "=========================================="
echo "Results saved in: $OUTPUT_DIR/"
echo ""
echo "Key files generated:"
echo "- summary_$TIMESTAMP.txt - Executive summary"
echo "- missing_files_$TIMESTAMP.txt - Files needing porting"
echo "- functions_analysis_$TIMESTAMP.txt - Function comparison"
echo "- action_items_$TIMESTAMP.txt - Prioritized next steps"
echo ""
echo "🔍 Review these files to identify missing code and functionality!"